use std::io::Seek;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::{fs, io, net};

use crate::collections::HashMap;
use crate::LocalTime;
use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

/// A map with the ability to randomly select values.
#[derive(Debug)]
pub struct AddressBook<K, V> {
    inner: HashMap<K, V>,
    rng: fastrand::Rng,
}

impl<K, V> AddressBook<K, V> {
    /// Create a new address book.
    pub fn new(rng: fastrand::Rng) -> Self {
        Self {
            inner: HashMap::with_hasher(rng.clone().into()),
            rng,
        }
    }

    /// Pick a random value in the book.
    pub fn sample(&self) -> Option<(&K, &V)> {
        self.sample_with(|_, _| true)
    }

    /// Pick a random value in the book matching a predicate.
    pub fn sample_with(&self, mut predicate: impl FnMut(&K, &V) -> bool) -> Option<(&K, &V)> {
        if let Some(pairs) = NonEmpty::from_vec(
            self.inner
                .iter()
                .filter(|(k, v)| predicate(*k, *v))
                .collect(),
        ) {
            let ix = self.rng.usize(..pairs.len());
            let pair = pairs[ix]; // Can't fail.

            Some(pair)
        } else {
            None
        }
    }

    /// Cycle through the keys at random. The random cycle repeats ad-infintum.
    pub fn cycle(&self) -> impl Iterator<Item = &K> {
        self.shuffled().map(|(k, _)| k).cycle()
    }

    /// Return a shuffled iterator over the keys.
    pub fn shuffled(&self) -> std::vec::IntoIter<(&K, &V)> {
        let mut keys = self.inner.iter().collect::<Vec<_>>();
        self.rng.shuffle(&mut keys);

        keys.into_iter()
    }
}

impl<K, V> Deref for AddressBook<K, V> {
    type Target = HashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<K, V> DerefMut for AddressBook<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// A known address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownAddress {
    /// Network address.
    pub addr: net::SocketAddr,
    /// Address of the peer who sent us this address.
    pub source: Source,
    /// Last time this address was used to successfully connect to a peer.
    #[serde(with = "local_time")]
    pub last_success: Option<LocalTime>,
    /// Last time this address was sampled.
    #[serde(with = "local_time")]
    pub last_sampled: Option<LocalTime>,
    /// Last time this address was tried.
    #[serde(with = "local_time")]
    pub last_attempt: Option<LocalTime>,
    /// Last time this peer was seen alive.
    #[serde(with = "local_time")]
    pub last_active: Option<LocalTime>,
}

impl KnownAddress {
    /// Create a new known address.
    pub fn new(addr: net::SocketAddr, source: Source, last_active: Option<LocalTime>) -> Self {
        Self {
            addr,
            source,
            last_success: None,
            last_attempt: None,
            last_sampled: None,
            last_active,
        }
    }
}

/// Address source. Specifies where an address originated from.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// An address that was shared by another peer.
    Peer(net::SocketAddr),
    /// An address that came from a DNS seed.
    Dns,
    /// An address that came from some source external to the system, eg.
    /// specified by the user or added directly to the address manager.
    Imported,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Peer(addr) => write!(f, "{}", addr),
            Self::Dns => write!(f, "DNS"),
            Self::Imported => write!(f, "Imported"),
        }
    }
}

/// A file-backed address cache.
#[derive(Debug)]
pub struct Cache {
    addrs: std::collections::HashMap<net::IpAddr, KnownAddress>,
    file: fs::File,
}

impl Cache {
    /// Open an existing cache.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .and_then(Self::from)
    }

    /// Create a new cache.
    pub fn create<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        use std::collections::HashMap;

        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;

        Ok(Self {
            file,
            addrs: HashMap::new(),
        })
    }

    /// Create a new cache from a file.
    pub fn from(mut file: fs::File) -> io::Result<Self> {
        use std::collections::HashMap;

        let bytes = file.seek(io::SeekFrom::End(0))?;
        let addrs = if bytes == 0 {
            HashMap::new()
        } else {
            file.rewind()?;
            serde_json::from_reader(&file)?
        };

        Ok(Self { file, addrs })
    }
}

impl Store for Cache {
    fn get_mut(&mut self, ip: &net::IpAddr) -> Option<&mut KnownAddress> {
        self.addrs.get_mut(ip)
    }

    fn get(&self, ip: &net::IpAddr) -> Option<&KnownAddress> {
        self.addrs.get(ip)
    }

    fn remove(&mut self, ip: &net::IpAddr) -> Option<KnownAddress> {
        self.addrs.remove(ip)
    }

    fn insert(&mut self, ip: net::IpAddr, ka: KnownAddress) -> bool {
        <std::collections::HashMap<_, _> as Store>::insert(&mut self.addrs, ip, ka)
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&net::IpAddr, &KnownAddress)> + 'a> {
        Box::new(self.addrs.iter())
    }

    fn clear(&mut self) {
        self.addrs.clear()
    }

    fn len(&self) -> usize {
        self.addrs.len()
    }

    fn flush<'a>(&mut self) -> io::Result<()> {
        use io::Write;

        let peers = serde_json::to_value(&self.addrs)?;
        let s = serde_json::to_string(&peers)?;

        self.file.set_len(0)?;
        self.file.seek(io::SeekFrom::Start(0))?;
        self.file.write_all(s.as_bytes())?;
        self.file.write_all(&[b'\n'])?;
        self.file.sync_data()?;

        Ok(())
    }
}

/// Address store.
///
/// Used to store peer addresses and metadata.
pub trait Store {
    /// Get a known peer address.
    fn get(&self, ip: &net::IpAddr) -> Option<&KnownAddress>;

    /// Get a known peer address mutably.
    fn get_mut(&mut self, ip: &net::IpAddr) -> Option<&mut KnownAddress>;

    /// Insert a *new* address into the store. Returns `true` if the address was inserted,
    /// or `false` if it was already known.
    fn insert(&mut self, ip: net::IpAddr, ka: KnownAddress) -> bool;

    /// Remove an address from the store.
    fn remove(&mut self, ip: &net::IpAddr) -> Option<KnownAddress>;

    /// Return an iterator over the known addresses.
    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&net::IpAddr, &KnownAddress)> + 'a>;

    /// Returns the number of addresses.
    fn len(&self) -> usize;

    /// Returns true if there are no addresses.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Seed the peer store with addresses.
    /// Fails if *none* of the seeds could be resolved to addresses.
    fn seed<S: net::ToSocketAddrs>(
        &mut self,
        seeds: impl Iterator<Item = S>,
        source: Source,
    ) -> io::Result<()> {
        let mut error = None;
        let mut success = false;

        for seed in seeds {
            match seed.to_socket_addrs() {
                Ok(addrs) => {
                    success = true;
                    for addr in addrs {
                        self.insert(addr.ip(), KnownAddress::new(addr, source, None));
                    }
                }
                Err(err) => error = Some(err),
            }
        }

        if success {
            return Ok(());
        }
        if let Some(err) = error {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("seeds failed to resolve: {}", err),
            ));
        }
        Ok(())
    }

    /// Clears the store of all addresses.
    fn clear(&mut self);

    /// Flush data to permanent storage.
    fn flush(&mut self) -> io::Result<()>;
}

/// Implementation of [`Store`] for [`std::collections::HashMap`].
impl Store for std::collections::HashMap<net::IpAddr, KnownAddress> {
    fn get_mut(&mut self, ip: &net::IpAddr) -> Option<&mut KnownAddress> {
        self.get_mut(ip)
    }

    fn get(&self, ip: &net::IpAddr) -> Option<&KnownAddress> {
        self.get(ip)
    }

    fn remove(&mut self, ip: &net::IpAddr) -> Option<KnownAddress> {
        self.remove(ip)
    }

    fn insert(&mut self, ip: net::IpAddr, ka: KnownAddress) -> bool {
        use std::collections::hash_map::Entry;

        match self.entry(ip) {
            Entry::Vacant(v) => {
                v.insert(ka);
            }
            Entry::Occupied(_) => return false,
        }
        true
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&net::IpAddr, &KnownAddress)> + 'a> {
        Box::new(self.iter())
    }

    fn clear(&mut self) {
        self.clear()
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Implementation of [`Store`] for [`crate::collections::HashMap`].
impl Store for crate::collections::HashMap<net::IpAddr, KnownAddress> {
    fn get_mut(&mut self, ip: &net::IpAddr) -> Option<&mut KnownAddress> {
        self.get_mut(ip)
    }

    fn get(&self, ip: &net::IpAddr) -> Option<&KnownAddress> {
        self.get(ip)
    }

    fn remove(&mut self, ip: &net::IpAddr) -> Option<KnownAddress> {
        self.remove(ip)
    }

    fn insert(&mut self, ip: net::IpAddr, ka: KnownAddress) -> bool {
        use std::collections::hash_map::Entry;

        match self.entry(ip) {
            Entry::Vacant(v) => {
                v.insert(ka);
            }
            Entry::Occupied(_) => return false,
        }
        true
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (&net::IpAddr, &KnownAddress)> + 'a> {
        Box::new(self.iter())
    }

    fn clear(&mut self) {
        self.clear()
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

mod local_time {
    use super::LocalTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<LocalTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Option<u64> = Deserialize::deserialize(deserializer)?;

        if let Some(value) = value {
            Ok(Some(LocalTime::from_secs(value)))
        } else {
            Ok(None)
        }
    }

    pub fn serialize<S>(value: &Option<LocalTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(local_time) = value {
            serializer.serialize_u64(local_time.as_secs())
        } else {
            serializer.serialize_none()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cache");

        Cache::create(&path).unwrap();
        let cache = Cache::open(&path).unwrap();

        assert!(cache.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cache");
        let mut expected = Vec::new();

        {
            let mut cache = Cache::create(&path).unwrap();

            for i in 32..48 {
                let ip = net::IpAddr::from([127, 0, 0, i]);
                let addr = net::SocketAddr::from((ip, 8333));
                let ka = KnownAddress {
                    addr,
                    source: Source::Dns,
                    last_success: Some(LocalTime::from_secs(i as u64)),
                    last_sampled: Some(LocalTime::from_secs((i + 1) as u64)),
                    last_attempt: None,
                    last_active: None,
                };
                cache.insert(ip, ka);
            }
            cache.flush().unwrap();

            for (ip, ka) in cache.iter() {
                expected.push((*ip, ka.clone()));
            }
        }

        {
            let cache = Cache::open(&path).unwrap();
            let mut actual = cache
                .iter()
                .map(|(i, ka)| (*i, ka.clone()))
                .collect::<Vec<_>>();

            actual.sort_by_key(|(i, _)| *i);
            expected.sort_by_key(|(i, _)| *i);

            assert_eq!(actual, expected);
        }
    }
}
