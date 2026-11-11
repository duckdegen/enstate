//! `NameNFT` events and the cache entries each of them makes stale.

use std::collections::BTreeSet;

use enstate_shared::core::gns::{reverse_key, subtree_tag, token_tag};
use ethers::types::{Address, Log, H256, U256};
use ethers_core::abi::{self, ParamType, Token};
use ethers_core::utils::keccak256;
use lazy_static::lazy_static;

fn topic(signature: &str) -> H256 {
    H256(keccak256(signature))
}

lazy_static! {
    static ref NAME_REGISTERED: H256 = topic("NameRegistered(uint256,string,address,uint256)");
    static ref SUBDOMAIN_REGISTERED: H256 = topic("SubdomainRegistered(uint256,uint256,string)");
    static ref NAME_RENEWED: H256 = topic("NameRenewed(uint256,uint256)");
    static ref PRIMARY_NAME_SET: H256 = topic("PrimaryNameSet(address,uint256)");
    static ref TRANSFER: H256 = topic("Transfer(address,address,uint256)");
    static ref ADDR_CHANGED: H256 = topic("AddrChanged(bytes32,address)");
    static ref ADDRESS_CHANGED: H256 = topic("AddressChanged(bytes32,uint256,bytes)");
    static ref CONTENTHASH_CHANGED: H256 = topic("ContenthashChanged(bytes32,bytes)");
    static ref TEXT_CHANGED: H256 = topic("TextChanged(bytes32,string,string)");
}

/// A `NameNFT` event that changes what a name or an address resolves to.
/// `node` is the namehash of the name concerned, which is its token id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GnsEvent {
    /// A name or subdomain was registered. Also emitted when an expired name
    /// is registered again or a subdomain is reclaimed by its parent's owner,
    /// in which case the old token is burned, its records are wiped and the
    /// previous owner's primary name is deleted, all without further events,
    /// and every subdomain below it goes stale.
    NameRegistered {
        node: H256,
        label: String,
        owner: Address,
        expires_at: U256,
    },
    /// Follows `NameRegistered` when the name is a subdomain.
    SubdomainRegistered {
        node: H256,
        parent: H256,
    },
    /// A top-level name was renewed, which also revives it and everything
    /// below it if it had run into its grace period.
    NameRenewed {
        node: H256,
        expires_at: U256,
    },
    /// An address picked (or, with the zero node, cleared) its primary name.
    PrimaryNameSet {
        address: Address,
        node: H256,
    },
    /// ERC-721 transfer. A name without an explicit address resolves to its
    /// owner, so this may change what the name resolves to.
    Transfer {
        from: Address,
        to: Address,
        node: H256,
    },
    AddrChanged {
        node: H256,
    },
    AddressChanged {
        node: H256,
    },
    ContenthashChanged {
        node: H256,
    },
    TextChanged {
        node: H256,
    },
}

/// Cache entries to drop: every entry registered under one of `tags`, plus
/// the entries stored under `keys`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invalidation {
    pub tags: BTreeSet<String>,
    pub keys: BTreeSet<String>,
}

impl Invalidation {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.keys.is_empty()
    }

    pub fn extend(&mut self, other: &Self) {
        self.tags.extend(other.tags.iter().cloned());
        self.keys.extend(other.keys.iter().cloned());
    }
}

fn address_from_topic(topic: &H256) -> Address {
    Address::from_slice(&topic.as_bytes()[12..])
}

impl GnsEvent {
    /// Decodes a log emitted by `NameNFT`. Events that do not affect
    /// resolution (`Committed`, approvals) yield `None`.
    pub fn decode(log: &Log) -> Option<Self> {
        let (signature, indexed) = log.topics.split_first()?;

        let event = if signature == &*NAME_REGISTERED {
            let [node, owner] = indexed else { return None };

            let mut data = abi::decode(&[ParamType::String, ParamType::Uint(256)], &log.data)
                .ok()?
                .into_iter();

            let (Some(Token::String(label)), Some(Token::Uint(expires_at))) =
                (data.next(), data.next())
            else {
                return None;
            };

            Self::NameRegistered {
                node: *node,
                label,
                owner: address_from_topic(owner),
                expires_at,
            }
        } else if signature == &*SUBDOMAIN_REGISTERED {
            let [node, parent] = indexed else { return None };

            Self::SubdomainRegistered {
                node: *node,
                parent: *parent,
            }
        } else if signature == &*NAME_RENEWED {
            let [node] = indexed else { return None };

            Self::NameRenewed {
                node: *node,
                expires_at: U256::from_big_endian(log.data.get(..32)?),
            }
        } else if signature == &*PRIMARY_NAME_SET {
            let [address, node] = indexed else {
                return None;
            };

            Self::PrimaryNameSet {
                address: address_from_topic(address),
                node: *node,
            }
        } else if signature == &*TRANSFER {
            let [from, to, node] = indexed else {
                return None;
            };

            Self::Transfer {
                from: address_from_topic(from),
                to: address_from_topic(to),
                node: *node,
            }
        } else {
            // the resolver events all carry the node as their first topic
            let node = *indexed.first()?;

            if signature == &*ADDR_CHANGED {
                Self::AddrChanged { node }
            } else if signature == &*ADDRESS_CHANGED {
                Self::AddressChanged { node }
            } else if signature == &*CONTENTHASH_CHANGED {
                Self::ContenthashChanged { node }
            } else if signature == &*TEXT_CHANGED {
                Self::TextChanged { node }
            } else {
                return None;
            }
        };

        Some(event)
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::NameRegistered { .. } => "NameRegistered",
            Self::SubdomainRegistered { .. } => "SubdomainRegistered",
            Self::NameRenewed { .. } => "NameRenewed",
            Self::PrimaryNameSet { .. } => "PrimaryNameSet",
            Self::Transfer { .. } => "Transfer",
            Self::AddrChanged { .. } => "AddrChanged",
            Self::AddressChanged { .. } => "AddressChanged",
            Self::ContenthashChanged { .. } => "ContenthashChanged",
            Self::TextChanged { .. } => "TextChanged",
        }
    }

    /// The cache entries this event makes stale.
    ///
    /// Everything cached about a name is registered under the name's token
    /// tag: its profile, and the primary name of every address whose
    /// `primaryName` is that token, whether it currently resolves back or
    /// not. Whatever is cached about a name's subdomains is registered under
    /// its subtree tag. So:
    ///
    /// * anything that changes the records or the owner of a name drops the
    ///   token tag: the profile changed, and addresses pointing at the name
    ///   may have gained or lost it as their primary name;
    /// * anything that changes whether a name is active (registration,
    ///   renewal, burn) drops the subtree tag as well, because subdomains are
    ///   active only as long as their whole parent chain is;
    /// * picking a primary name only concerns the address that did.
    pub fn invalidation(&self) -> Invalidation {
        let mut invalidation = Invalidation::default();

        match self {
            Self::NameRegistered { node, .. }
            | Self::SubdomainRegistered { node, .. }
            | Self::NameRenewed { node, .. } => {
                invalidation.tags.insert(token_tag(node));
                invalidation.tags.insert(subtree_tag(node));
            }
            Self::Transfer { to, node, .. } => {
                invalidation.tags.insert(token_tag(node));

                // a burn is the first half of a re-registration
                if to.is_zero() {
                    invalidation.tags.insert(subtree_tag(node));
                }
            }
            Self::AddrChanged { node }
            | Self::AddressChanged { node }
            | Self::ContenthashChanged { node }
            | Self::TextChanged { node } => {
                invalidation.tags.insert(token_tag(node));
            }
            Self::PrimaryNameSet { address, .. } => {
                invalidation.keys.insert(reverse_key(address));
            }
        }

        invalidation
    }
}

#[cfg(test)]
mod tests {
    use ethers::providers::namehash;
    use ethers::types::Bytes;

    use super::*;

    fn log(topics: &[&str], data: &str) -> Log {
        Log {
            topics: topics.iter().map(|topic| topic.parse().unwrap()).collect(),
            data: data.parse::<Bytes>().unwrap(),
            ..Log::default()
        }
    }

    fn tags(invalidation: &Invalidation) -> Vec<&str> {
        invalidation.tags.iter().map(String::as_str).collect()
    }

    const NODE: &str = "0x41593da8bfd4bccd6b82a0f4edd9adacf5a92212c6238c69b4704fd763bcc162";
    const ALICE: &str = "0x000000000000000000000000bbbce157ecbb945b1f92af88413b911910c727c8";
    const BOB: &str = "0x000000000000000000000000e8764981b6869ea657bef673539ee07142205ae8";
    const ZERO: &str = "0x0000000000000000000000000000000000000000000000000000000000000000";

    fn node_tags(token: bool, subtree: bool) -> Vec<String> {
        let node = namehash("gns.gwei");

        assert_eq!(node, NODE.parse().unwrap());

        let mut tags = Vec::new();
        if subtree {
            tags.push(subtree_tag(&node));
        }
        if token {
            tags.push(token_tag(&node));
        }
        tags
    }

    #[test]
    fn test_topics_match_mainnet_logs() {
        // topic0 values as seen in NameNFT logs on Ethereum mainnet
        for (topic, expected) in [
            (
                *NAME_REGISTERED,
                "0xe9b829b8676c143f46f458c54ab9eedea80dc58eaec9c3df7e0134aabf7f9d28",
            ),
            (
                *SUBDOMAIN_REGISTERED,
                "0x380897ee9752efc3f5bd42b9d305b9445504e347e953bd955691a654c20f6b58",
            ),
            (
                *TRANSFER,
                "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
            ),
            (
                *ADDR_CHANGED,
                "0x52d7d861f09ab3d26239d492e8968629f95e9e318cf0b73bfddc441522a15fd2",
            ),
            (
                *CONTENTHASH_CHANGED,
                "0xe379c1624ed7e714cc0937528a32359d69d5281337765313dba4e081b72d7578",
            ),
        ] {
            assert_eq!(topic, expected.parse().unwrap());
        }
    }

    // the registration of safeq.gwei and safe.safeq.gwei on Ethereum mainnet
    // (blocks 26386606 and 26386612)
    #[test]
    fn test_decode_mainnet_logs() {
        let owner = "0x00000000000000000000000016a20a40485d9dfc0789de97e0bf9b17ba8edd23";
        let safeq = namehash("safeq.gwei");
        let safe = namehash("safe.safeq.gwei");

        assert_eq!(
            GnsEvent::decode(&log(
                &[
                    &format!("{:?}", *TRANSFER),
                    ZERO,
                    owner,
                    &format!("{safeq:?}")
                ],
                "0x"
            )),
            Some(GnsEvent::Transfer {
                from: Address::zero(),
                to: address_from_topic(&owner.parse().unwrap()),
                node: safeq,
            })
        );

        assert_eq!(
            GnsEvent::decode(&log(
                &[
                    "0xe9b829b8676c143f46f458c54ab9eedea80dc58eaec9c3df7e0134aabf7f9d28",
                    "0xe8aa1854f17015768197d4bedbdc1f0a0ce9c5df985e76950047f128126f9e1a",
                    owner,
                ],
                "0x0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000006cd4a3e300000000000000000000000000000000000000000000000000000000000000057361666571000000000000000000000000000000000000000000000000000000",
            )),
            Some(GnsEvent::NameRegistered {
                node: safeq,
                label: "safeq".to_string(),
                owner: address_from_topic(&owner.parse().unwrap()),
                expires_at: 0x6cd4_a3e3u64.into(),
            })
        );

        assert_eq!(
            GnsEvent::decode(&log(
                &[
                    "0x380897ee9752efc3f5bd42b9d305b9445504e347e953bd955691a654c20f6b58",
                    "0xbb4f098b076a2630600e758c5cd291192e2969d4f283dba0e20c42bcfb0bccca",
                    "0xe8aa1854f17015768197d4bedbdc1f0a0ce9c5df985e76950047f128126f9e1a",
                ],
                "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000047361666500000000000000000000000000000000000000000000000000000000",
            )),
            Some(GnsEvent::SubdomainRegistered {
                node: safe,
                parent: safeq,
            })
        );

        // Committed(bytes32,address): not a resolution change
        assert_eq!(
            GnsEvent::decode(&log(
                &[
                    "0xfda886b5d38d5d61e432b3acc9f19640a2b8c83c87a9c584a780d3854b309b9c",
                    "0x94d1b98f208baea1b8edad2f2218b47b29cdf9c657ac60eaf51f2a6ddfa8afe2",
                    owner,
                ],
                "0x",
            )),
            None
        );
    }

    #[test]
    fn test_name_registered() {
        let label = abi::encode(&[
            Token::String("gns".to_string()),
            Token::Uint(1_814_160_359u64.into()),
        ]);
        let event = GnsEvent::decode(&log(
            &[&format!("{:?}", *NAME_REGISTERED), NODE, ALICE],
            &format!("0x{}", hex::encode(label)),
        ))
        .unwrap();

        assert_eq!(
            event,
            GnsEvent::NameRegistered {
                node: NODE.parse().unwrap(),
                label: "gns".to_string(),
                owner: "0xbbbce157ecbb945b1f92af88413b911910c727c8"
                    .parse()
                    .unwrap(),
                expires_at: 1_814_160_359u64.into(),
            }
        );

        // (re-)registration: the name itself and everything below it
        let invalidation = event.invalidation();
        assert_eq!(tags(&invalidation), node_tags(true, true));
        assert!(invalidation.keys.is_empty());
    }

    #[test]
    fn test_subdomain_registered_and_renewed() {
        let parent = "0xe8aa1854f1700000000000000000000000000000000000000000000000000000";

        let event = GnsEvent::decode(&log(
            &[&format!("{:?}", *SUBDOMAIN_REGISTERED), NODE, parent],
            "0x",
        ))
        .unwrap();

        assert_eq!(
            event,
            GnsEvent::SubdomainRegistered {
                node: NODE.parse().unwrap(),
                parent: parent.parse().unwrap(),
            }
        );
        // nothing cached about the parent changes
        assert_eq!(tags(&event.invalidation()), node_tags(true, true));

        let event = GnsEvent::decode(&log(
            &[&format!("{:?}", *NAME_RENEWED), NODE],
            "0x000000000000000000000000000000000000000000000000000000006c21e3e7",
        ))
        .unwrap();

        assert_eq!(
            event,
            GnsEvent::NameRenewed {
                node: NODE.parse().unwrap(),
                expires_at: 0x6c21_e3e7u64.into(),
            }
        );
        // renewal revives the name and its subdomains
        assert_eq!(tags(&event.invalidation()), node_tags(true, true));
    }

    #[test]
    fn test_transfer() {
        let transfer = |from, to| {
            GnsEvent::decode(&log(&[&format!("{:?}", *TRANSFER), from, to, NODE], "0x")).unwrap()
        };

        let event = transfer(ALICE, BOB);
        assert_eq!(
            event,
            GnsEvent::Transfer {
                from: address_from_topic(&ALICE.parse().unwrap()),
                to: address_from_topic(&BOB.parse().unwrap()),
                node: NODE.parse().unwrap(),
            }
        );

        // a transfer changes what the name resolves to, not its subdomains
        let invalidation = event.invalidation();
        assert_eq!(tags(&invalidation), node_tags(true, false));
        // reverse entries of both parties hang off the token tag, if they
        // point at this name at all
        assert!(invalidation.keys.is_empty());

        // mint
        assert_eq!(
            tags(&transfer(ZERO, BOB).invalidation()),
            node_tags(true, false)
        );
        // burn (re-registration): subdomains go stale
        assert_eq!(
            tags(&transfer(ALICE, ZERO).invalidation()),
            node_tags(true, true)
        );
    }

    #[test]
    fn test_primary_name_set() {
        let event = GnsEvent::decode(&log(
            &[&format!("{:?}", *PRIMARY_NAME_SET), ALICE, NODE],
            "0x",
        ))
        .unwrap();

        // only the address that picked the name is affected
        let invalidation = event.invalidation();
        assert!(invalidation.tags.is_empty());
        assert_eq!(
            invalidation.keys.into_iter().collect::<Vec<_>>(),
            vec!["gwei:a:0xbbbce157ecbb945b1f92af88413b911910c727c8"]
        );

        // clearing (token 0) is decoded the same way
        let event = GnsEvent::decode(&log(
            &[&format!("{:?}", *PRIMARY_NAME_SET), ALICE, ZERO],
            "0x",
        ))
        .unwrap();
        assert_eq!(event.invalidation().keys.len(), 1);
    }

    #[test]
    fn test_record_events() {
        for (topic, name) in [
            (*ADDR_CHANGED, "AddrChanged"),
            (*ADDRESS_CHANGED, "AddressChanged"),
            (*CONTENTHASH_CHANGED, "ContenthashChanged"),
            (*TEXT_CHANGED, "TextChanged"),
        ] {
            // TextChanged carries the hash of its key as a second topic
            let event = GnsEvent::decode(&log(&[&format!("{topic:?}"), NODE, BOB], "0x")).unwrap();

            assert_eq!(event.name(), name);

            let invalidation = event.invalidation();
            assert_eq!(tags(&invalidation), node_tags(true, false));
            assert!(invalidation.keys.is_empty());
        }
    }

    #[test]
    fn test_irrelevant_logs() {
        // Committed(bytes32,address)
        let committed = format!("{:?}", topic("Committed(bytes32,address)"));
        assert_eq!(
            GnsEvent::decode(&log(&[&committed, NODE, ALICE], "0x")),
            None
        );
        // Approval(address,address,uint256)
        let approval = format!("{:?}", topic("Approval(address,address,uint256)"));
        assert_eq!(
            GnsEvent::decode(&log(&[&approval, ALICE, BOB, NODE], "0x")),
            None
        );
        // malformed
        assert_eq!(GnsEvent::decode(&log(&[], "0x")), None);
        assert_eq!(
            GnsEvent::decode(&log(&[&format!("{:?}", *TRANSFER), ALICE], "0x")),
            None
        );
        assert_eq!(
            GnsEvent::decode(&log(
                &[&format!("{:?}", *NAME_REGISTERED), NODE, ALICE],
                "0x"
            )),
            None
        );
    }

    #[test]
    fn test_invalidation_extend() {
        let mut invalidation = Invalidation::default();
        assert!(invalidation.is_empty());

        invalidation.extend(
            &GnsEvent::AddrChanged {
                node: NODE.parse().unwrap(),
            }
            .invalidation(),
        );
        invalidation.extend(
            &GnsEvent::NameRenewed {
                node: NODE.parse().unwrap(),
                expires_at: U256::zero(),
            }
            .invalidation(),
        );

        assert_eq!(tags(&invalidation), node_tags(true, true));
    }
}
