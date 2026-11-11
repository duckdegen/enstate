use std::collections::BTreeMap;
use std::sync::Arc;

use ethers::prelude::Http;
use ethers::providers::Provider;
use ethers_ccip_read::CCIPReadMiddleware;
use ethers_core::types::H160;
use serde::{Deserialize, Serialize};

use crate::core::lookup_data::NameSystem;
use crate::core::resolvers::gns::GnsContracts;
use crate::models::multicoin::cointype::coins::CoinType;
use crate::utils::eip55::EIP55Address;
use crate::utils::factory::Factory;

pub mod address;
pub mod error;
pub mod gns;
pub mod lookup_data;
pub mod profile;
pub mod records;
pub mod resolvers;

pub type CCIPProvider = CCIPReadMiddleware<Arc<Provider<Http>>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    // Name
    pub name: String,
    // Ethereum Mainnet Address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<EIP55Address>,
    // Avatar URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    // Header URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    // Preferred Capitalization of Name
    pub display: String,
    // Content Hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contenthash: Option<String>,
    // Records
    pub records: BTreeMap<String, String>,
    // Addresses on different chains
    pub chains: BTreeMap<String, String>,
    // Unix Timestamp of date it was loaded
    pub fresh: i64,
    // Resolver the information was fetched from
    pub resolver: EIP55Address,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ccip_urls: Vec<String>,
    // Errors encountered while fetching & decoding
    pub errors: BTreeMap<String, String>,
}

impl Profile {
    /// Whether the name has no address and no record of any kind, which is
    /// what a name nobody registered resolves to.
    pub fn is_blank(&self) -> bool {
        self.address.is_none()
            && self.avatar.is_none()
            && self.header.is_none()
            && self.contenthash.is_none()
            && self.records.is_empty()
            && self.chains.is_empty()
    }
}

pub struct ENSService {
    pub cache: Box<dyn crate::cache::CacheLayer>,
    pub discovery: Option<Box<dyn crate::discovery::Discovery>>,
    pub rpc: Box<dyn Factory<Arc<Provider<Http>>>>,
    pub opensea_api_key: String,
    pub ipfs_gateway: String,
    pub arweave_gateway: String,
    pub profile_records: Arc<[String]>,
    pub profile_chains: Arc<[CoinType]>,
    pub universal_resolver: H160,
    pub gns: GnsContracts,
    /// The order name systems are preferred in when an address has a primary
    /// name in more than one of them.
    pub name_services_order: Vec<NameSystem>,
    pub max_bulk_size: usize,
    pub cache_ttl: Option<u32>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ethers_core::types::H160;

    use super::Profile;
    use crate::utils::eip55::EIP55Address;

    fn blank_profile() -> Profile {
        Profile {
            name: "name.eth".to_string(),
            address: None,
            avatar: None,
            header: None,
            display: "name.eth".to_string(),
            contenthash: None,
            records: BTreeMap::new(),
            chains: BTreeMap::new(),
            fresh: 0,
            resolver: EIP55Address(H160::repeat_byte(1)),
            ccip_urls: vec![],
            errors: BTreeMap::new(),
        }
    }

    #[test]
    fn profile_without_any_record_is_blank() {
        assert!(blank_profile().is_blank());
    }

    #[test]
    fn profile_with_an_address_or_a_record_is_not_blank() {
        let mut with_address = blank_profile();
        with_address.address = Some(EIP55Address(H160::repeat_byte(2)));
        assert!(!with_address.is_blank());

        let mut with_record = blank_profile();
        with_record
            .records
            .insert("com.twitter".to_string(), "name".to_string());
        assert!(!with_record.is_blank());

        let mut with_chain = blank_profile();
        with_chain
            .chains
            .insert("btc".to_string(), "bc1q".to_string());
        assert!(!with_chain.is_blank());
    }
}
