use ethers::prelude::Address;
use thiserror::Error;

use crate::core::gns::{GweiName, NameError};

#[derive(Debug)]
pub enum LookupInfo {
    Name(String),
    Address(Address),
}

#[derive(Error, Clone, Debug)]
pub enum NameParseError {
    #[error("Invalid name format")]
    InvalidNameFormat,
}

impl LookupInfo {
    pub fn guess<T: AsRef<str>>(name_or_address: T) -> Result<LookupInfo, NameParseError> {
        let name_or_address = name_or_address.as_ref();

        if let Ok(address) = name_or_address.parse::<Address>() {
            return Ok(LookupInfo::Address(address));
        }

        if !crate::patterns::test_domain(name_or_address) {
            return Err(NameParseError::InvalidNameFormat);
        }

        Ok(LookupInfo::Name(name_or_address.to_lowercase()))
    }
}

/// The name systems names are served from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameSystem {
    /// The Ethereum Name Service, through its universal resolver.
    Ens,
    /// The Gwei Name Service (`.gwei`), through its `NameNFT` contract.
    Gwei,
}

impl NameSystem {
    /// Where the primary name of an address is looked for, most preferred
    /// first: an address with a primary name in both systems is known by its
    /// `.gwei` name.
    pub const DEFAULT_ORDER: [NameSystem; 2] = [NameSystem::Gwei, NameSystem::Ens];

    /// Parses an order of name systems, given either as a comma-separated
    /// list (`GWEI,ENS`) or as a JSON array (`["ENS", "GWEI"]`), in any case.
    /// A system that is not listed comes after the ones that are; a value
    /// that is not a name system is an error.
    pub fn parse_order(raw: &str) -> Result<Vec<NameSystem>, String> {
        let raw = raw.trim();

        let listed: Vec<String> = if raw.starts_with('[') {
            serde_json::from_str(raw)
                .map_err(|error| format!("not a JSON array of strings ({error})"))?
        } else {
            raw.split(',').map(ToString::to_string).collect()
        };

        let mut order = Vec::new();

        for system in listed.iter().map(|it| it.trim()) {
            let system = match system.to_ascii_uppercase().as_str() {
                "ENS" => NameSystem::Ens,
                "GWEI" => NameSystem::Gwei,
                // `GWEI,` and an empty value list nothing
                "" => continue,
                _ => {
                    return Err(format!(
                        "unknown name service '{system}', expected ENS or GWEI"
                    ))
                }
            };

            if order.contains(&system) {
                return Err(format!("{system:?} is listed more than once"));
            }

            order.push(system);
        }

        for system in Self::DEFAULT_ORDER {
            if !order.contains(&system) {
                order.push(system);
            }
        }

        Ok(order)
    }
}

/// A name, by the system that serves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Name {
    Ens(String),
    Gwei(GweiName),
}

impl Name {
    /// Names ending in `.gwei` belong to the Gwei Name Service, every other
    /// name to ENS. A `.gwei` name that cannot exist is an error rather than
    /// an ENS name.
    pub fn parse(name: &str) -> Result<Name, NameError> {
        match GweiName::parse(name) {
            Ok(name) => Ok(Name::Gwei(name)),
            Err(NameError::NotGwei) => Ok(Name::Ens(name.to_string())),
            Err(error) => Err(error),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Name::Ens(name) => name,
            Name::Gwei(name) => name.as_str(),
        }
    }

    pub fn system(&self) -> NameSystem {
        match self {
            Name::Ens(_) => NameSystem::Ens,
            Name::Gwei(_) => NameSystem::Gwei,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_systems() {
        for name in ["gns.gwei", "sub.gns.gwei", "GNS.GWEI"] {
            assert_eq!(Name::parse(name).unwrap().system(), NameSystem::Gwei);
        }

        // everything else is left to ENS, untouched
        for name in [
            "vitalik.eth",
            "luc.computer",
            "gwei.eth",
            "foo.gwei.eth",
            "gwei",
            "Foo.ETH",
        ] {
            assert_eq!(Name::parse(name), Ok(Name::Ens(name.to_string())));
        }

        // a malformed .gwei name is not handed to ENS
        assert_eq!(Name::parse("a..gwei"), Err(NameError::InvalidLabel));

        assert_eq!(Name::parse("GNS.gwei").unwrap().as_str(), "gns.gwei");
    }

    #[test]
    fn test_parse_order() {
        use NameSystem::{Ens, Gwei};

        assert_eq!(NameSystem::DEFAULT_ORDER, [Gwei, Ens]);

        // comma-separated or a JSON array, in any case
        for raw in [
            "GWEI,ENS",
            "gwei,ens",
            " Gwei , ENS ",
            r#"["GWEI", "ENS"]"#,
            r#"["gwei","ens"]"#,
        ] {
            assert_eq!(NameSystem::parse_order(raw), Ok(vec![Gwei, Ens]), "{raw}");
        }

        for raw in [
            "ENS,GWEI",
            "ens, gwei",
            r#"["ENS", "GWEI"]"#,
            r#" [ "Ens" , "Gwei" ] "#,
        ] {
            assert_eq!(NameSystem::parse_order(raw), Ok(vec![Ens, Gwei]), "{raw}");
        }

        // a system that is not listed comes after the listed ones
        assert_eq!(NameSystem::parse_order("ENS"), Ok(vec![Ens, Gwei]));
        assert_eq!(NameSystem::parse_order(r#"["ENS"]"#), Ok(vec![Ens, Gwei]));
        assert_eq!(NameSystem::parse_order("gwei"), Ok(vec![Gwei, Ens]));
        assert_eq!(NameSystem::parse_order("ENS,"), Ok(vec![Ens, Gwei]));
        assert_eq!(NameSystem::parse_order(""), Ok(vec![Gwei, Ens]));
        assert_eq!(NameSystem::parse_order("[]"), Ok(vec![Gwei, Ens]));

        // anything else is refused rather than ignored
        assert_eq!(
            NameSystem::parse_order("ENS,DNS"),
            Err("unknown name service 'DNS', expected ENS or GWEI".to_string())
        );
        assert!(NameSystem::parse_order(r#"["ENS", "gns"]"#).is_err());
        assert!(NameSystem::parse_order("ens;gwei").is_err());
        assert!(NameSystem::parse_order("ENS,ens").is_err());
        assert!(NameSystem::parse_order(r#"["ENS", 1]"#).is_err());
        assert!(NameSystem::parse_order(r#"["ENS""#).is_err());
        assert!(NameSystem::parse_order(r#"{"ENS": 1}"#).is_err());
    }
}
