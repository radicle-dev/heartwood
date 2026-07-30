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
