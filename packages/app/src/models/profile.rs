use std::collections::BTreeMap;

use enstate_shared::core::Profile;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct ENSProfile {
    // Name
    #[schema(example = "gns.gwei")]
    pub name: String,
    // Ethereum Mainnet Address
    #[schema(example = "0xbBBCe157ecBB945b1F92Af88413b911910C727c8")]
    pub address: Option<String>,
    // Avatar URL
    #[schema(example = "https://cloudflare-ipfs.com/ipfs/bafkreifnrjhkl7ccr2ifwn2n7ap6dh2way25a6w5x2szegvj5pt4b5nvfu")]
    pub avatar: Option<String>,
    // Preferred Capitalization of Name
    #[schema(example = "GNS.gwei")]
    pub display: String,
    // Content Hash
    #[schema(example = "ipfs://bafybeidnycldkehcy6xixzqg72vad6pitav4lk5np3ev6tr6titlkvfpvi")]
    pub contenthash: Option<String>,
    // Records
    #[schema(example = "{\"world\":\"hello\"}")]
    pub records: BTreeMap<String, String>,
    // Addresses on different chains
    pub chains: BTreeMap<String, String>,
    // Unix Timestamp of date it was loaded
    #[schema(example = "1713363899484")]
    pub fresh: i64,
    // Resolver the information was fetched from
    #[schema(example = "0x9D51D507BC7264d4fE8Ad1cf7Fe191933A0a81d6")]
    pub resolver: String,
    // Errors encountered while fetching & decoding
    pub errors: BTreeMap<String, String>,
}

impl From<Profile> for ENSProfile {
    fn from(profile: Profile) -> Self {
        Self {
            name: profile.name,
            address: profile.address.map(|a| a.to_string()),
            avatar: profile.avatar,
            display: profile.display,
            contenthash: profile.contenthash,
            records: profile.records,
            chains: profile.chains,
            fresh: profile.fresh,
            resolver: profile.resolver.to_string(),
            errors: profile.errors,
        }
    }
}
