use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct License {
    pub name: String,
    pub url: String,
    pub attribution_required: bool,
    pub share_alike: bool,
}

impl License {
    pub fn from_commons(
        name: impl Into<String>,
        url: impl Into<String>,
    ) -> Result<Self, LicenseError> {
        let name = name.into();
        let normalized = name.to_ascii_lowercase();
        let is_public_domain = normalized.contains("public domain")
            || normalized == "cc0"
            || normalized.starts_with("cc0 ");
        let is_creative_commons = normalized.starts_with("cc by")
            || normalized.starts_with("creative commons attribution");

        if !is_public_domain && !is_creative_commons {
            return Err(LicenseError::Unsupported(name));
        }

        Ok(Self {
            share_alike: normalized.contains("share-alike")
                || normalized.contains("sharealike")
                || normalized.contains("by-sa"),
            attribution_required: !is_public_domain,
            name,
            url: url.into(),
        })
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LicenseError {
    #[error("unsupported media license: {0}")]
    Unsupported(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_public_domain_media() {
        let license = License::from_commons("Public domain", "https://example.com/pd").unwrap();
        assert!(!license.attribution_required);
        assert!(!license.share_alike);
    }

    #[test]
    fn keeps_share_alike_obligations() {
        let license = License::from_commons("CC BY-SA 4.0", "https://example.com/by-sa").unwrap();
        assert!(license.attribution_required);
        assert!(license.share_alike);
    }

    #[test]
    fn rejects_unknown_or_non_free_media() {
        let error =
            License::from_commons("All rights reserved", "https://example.com").unwrap_err();
        assert_eq!(
            error,
            LicenseError::Unsupported("All rights reserved".into())
        );
    }
}
