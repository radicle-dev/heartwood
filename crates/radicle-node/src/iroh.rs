pub(crate) mod presets {
    use iroh::address_lookup::{PkarrPublisher, PkarrResolver};
    use iroh::{RelayMode, RelayUrl};
    use url::Url;

    pub(crate) struct Radicle;

    impl Radicle {
        const DOMAIN: &str = "iroh.radicle.network";

        fn https(instance: usize, region: &str, service: &str, path: &str) -> Url {
            format!(
                "https://{instance}.{region}.{service}.{}{path}",
                Self::DOMAIN
            )
            .parse()
            .expect("valid URL")
        }

        /// Produces a [`RelayUrl`] for the given instance and region.
        fn relay(instance: usize, region: &str) -> RelayUrl {
            let url = Radicle::https(instance, region, "relay", "/");
            RelayUrl::from(url)
        }

        /// Produces a [`Url`] for the given instance and region for the DNS service.
        fn pkarr(instance: usize, region: &str) -> Url {
            Radicle::https(instance, region, "dns", "/pkarr")
        }
    }

    impl iroh::endpoint::presets::Preset for Radicle {
        fn apply(self, mut builder: iroh::endpoint::Builder) -> iroh::endpoint::Builder {
            // Set up relays.
            {
                let relays: [RelayUrl; 2] = [Radicle::relay(1, "eu"), Radicle::relay(1, "us")];

                builder = builder.relay_mode(RelayMode::Custom(iroh::RelayMap::from_iter(relays)));
            }

            // Set up Pkarr.
            {
                let pkarr: [Url; 2] = [Radicle::pkarr(1, "eu"), Radicle::pkarr(1, "us")];

                builder = iroh::endpoint::presets::Minimal.apply(builder);

                let publish_eu = PkarrPublisher::builder(pkarr[0].clone());
                let publish_us = PkarrPublisher::builder(pkarr[1].clone());

                let resolve_eu = PkarrResolver::builder(pkarr[0].clone());
                let resolve_us = PkarrResolver::builder(pkarr[1].clone());

                builder = builder.address_lookup(publish_eu);
                builder = builder.address_lookup(publish_us);
                builder = builder.address_lookup(resolve_eu);
                builder = builder.address_lookup(resolve_us);
            }

            // Set up mDNS.
            #[cfg(feature = "iroh-mdns-address-lookup")]
            {
                let mdns = iroh_mdns_address_lookup::MdnsAddressLookup::builder()
                    .service_name("radicle-node");

                builder = builder.address_lookup(mdns);

                // TODO: Consider filtering addresses via `builder.addr_filter(…)`.
            }

            // TODO: Set up mainline address lookup via crate `iroh-mainline-address-lookup`,
            // blocked by <https://github.com/n0-computer/iroh-address-lookups/issues/11>.

            builder
        }
    }
}

pub(crate) mod address_discovery {
    use std::collections::HashMap;
    use std::net;
    use std::sync::mpsc;

    use iroh::EndpointAddr;
    use iroh::address_lookup::memory::MemoryLookup;
    use radicle::node::address::Store as _;
    use radicle::node::events::Emitter;
    use radicle::node::{Address, Event, NodeId};

    /// Makes addresses learned by the Radicle node available to iroh.
    pub(crate) struct AddressDiscovery {
        lookup: MemoryLookup,
        events: mpsc::Receiver<Event>,
    }

    impl AddressDiscovery {
        /// Initialize the lookup from the address book, falling back to the
        /// network's bootstrap nodes when the book contains no addresses.
        pub(crate) async fn new(
            db: &radicle::node::Database,
            network: radicle::node::config::Network,
            emitter: &Emitter<Event>,
        ) -> Result<Self, radicle::node::address::Error> {
            // Subscribe before reading the database so that announcements
            // emitted while initialization is in progress aren't missed.
            let events = emitter.subscribe();
            let lookup = MemoryLookup::with_provenance("radicle");
            let mut addresses = HashMap::<NodeId, Vec<Address>>::new();

            for entry in db.entries()? {
                addresses
                    .entry(entry.node)
                    .or_default()
                    .push(entry.address.addr);
            }
            if addresses.is_empty() {
                for (_, _, bootstrap) in network.bootstrap() {
                    for address in bootstrap {
                        let (nid, address) = address.into_pair();
                        addresses.entry(nid).or_default().push(address);
                    }
                }
            }
            for (nid, addresses) in addresses {
                add(&lookup, nid, &addresses).await;
            }

            Ok(Self { lookup, events })
        }

        /// Return the lookup to register with the iroh endpoint.
        pub(crate) fn lookup(&self) -> MemoryLookup {
            self.lookup.clone()
        }

        /// Add all node addresses waiting on the event subscription.
        pub(crate) async fn update(&self) {
            while let Ok(event) = self.events.try_recv() {
                if let Event::NodeAnnounced { nid, addresses, .. } = event {
                    add(&self.lookup, nid, &addresses).await;
                }
            }
        }
    }

    async fn add(lookup: &MemoryLookup, nid: NodeId, addresses: &[Address]) {
        let id = match iroh::PublicKey::from_bytes(std::borrow::Borrow::borrow(&nid)) {
            Ok(id) => id,
            Err(err) => {
                log::warn!(target: "node", "Ignoring addresses for invalid node ID {nid}: {err}");
                return;
            }
        };
        let mut endpoint = EndpointAddr::new(id);

        for address in addresses {
            match address {
                Address::Ipv4 { host, port } => {
                    endpoint = endpoint
                        .with_ip_addr(net::SocketAddr::V4(net::SocketAddrV4::new(*host, *port)));
                }
                Address::Ipv6 { host, port, .. } => {
                    endpoint = endpoint.with_ip_addr(net::SocketAddr::V6(net::SocketAddrV6::new(
                        *host, *port, 0, 0,
                    )));
                }
                Address::Dns { host, port } => {
                    match tokio::net::lookup_host((host.as_str(), *port)).await {
                        Ok(addresses) => {
                            endpoint = endpoint.with_addrs(addresses.map(iroh::TransportAddr::Ip));
                        }
                        Err(err) => {
                            log::debug!(target: "node", "Unable to resolve direct address {host}:{port}: {err}");
                        }
                    }
                }
                #[cfg(feature = "tor")]
                address @ Address::Tor { .. } => {
                    log::warn!(target: "node", "Tor transport is not yet supported. Ignoring address '{address}'.");
                }
                #[cfg(feature = "i2p")]
                address @ Address::I2p { .. } => {
                    log::warn!(target: "node", "I2P transport is not yet supported. Ignoring address '{address}'.");
                }
                Address::Iroh => {}
                address => {
                    log::warn!(target: "node", "Ignoring unsupported address '{address}'.");
                }
            }
        }

        lookup.add_endpoint_info(endpoint);
    }
}
