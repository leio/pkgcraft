use std::collections::HashMap;
use std::str::FromStr;
use std::{fs, io};

use itertools::Itertools;
use scallop::{functions, variables};
use strum::{AsRefStr, Display, EnumString};
use tracing::warn;

use crate::dep::{self, Cpv, Dep, DepSet, Uri};
use crate::eapi::Eapi;
use crate::macros::build_from_paths;
use crate::pkgsh::{get_build_mut, source_ebuild, BuildData};
use crate::repo::{ebuild::Repo as EbuildRepo, Repository};
use crate::types::OrderedSet;
use crate::Error;

#[derive(AsRefStr, EnumString, Display, Debug, PartialEq, Eq, Hash, Copy, Clone)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Key {
    Iuse,
    RequiredUse,
    Depend,
    Rdepend,
    Pdepend,
    Bdepend,
    Idepend,
    Properties,
    Restrict,
    Description,
    Slot,
    DefinedPhases,
    Eapi,
    Homepage,
    Inherit,
    Inherited,
    Keywords,
    License,
    SrcUri,
}

impl Key {
    pub(crate) fn get(&self, eapi: &'static Eapi) -> Option<String> {
        match self {
            Key::DefinedPhases => {
                let mut phase_names: Vec<_> = eapi
                    .phases()
                    .iter()
                    .filter_map(|p| functions::find(p).map(|_| p.short_name()))
                    .collect();
                if phase_names.is_empty() {
                    None
                } else {
                    phase_names.sort_unstable();
                    Some(phase_names.join(" "))
                }
            }
            Key::Inherit => {
                let inherit = &get_build_mut().inherit;
                if inherit.is_empty() {
                    None
                } else {
                    Some(inherit.iter().join(" "))
                }
            }
            key => variables::optional(key).map(|s| s.split_whitespace().join(" ")),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Metadata {
    description: String,
    slot: String,
    deps: HashMap<Key, DepSet<Dep>>,
    license: Option<DepSet<String>>,
    properties: Option<DepSet<String>>,
    required_use: Option<DepSet<String>>,
    restrict: Option<DepSet<String>>,
    src_uri: Option<DepSet<Uri>>,
    homepage: OrderedSet<String>,
    defined_phases: OrderedSet<String>,
    keywords: OrderedSet<String>,
    iuse: OrderedSet<String>,
    inherit: OrderedSet<String>,
    inherited: OrderedSet<String>,
}

fn split(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split_whitespace().map(String::from)
}

impl Metadata {
    /// Convert raw metadata key value to stored value.
    fn convert(&mut self, eapi: &'static Eapi, key: Key, val: &str) -> crate::Result<()> {
        use Key::*;
        match key {
            Description => self.description = val.to_string(),
            Slot => self.slot = val.to_string(),
            Depend | Bdepend | Idepend | Rdepend | Pdepend => {
                if let Some(val) = dep::parse::dependencies(val, eapi)
                    .map_err(|e| Error::InvalidValue(format!("invalid {key}: {e}")))?
                {
                    self.deps.insert(key, val);
                }
            }
            License => self.license = dep::parse::license(val)?,
            Properties => self.properties = dep::parse::properties(val)?,
            RequiredUse => self.required_use = dep::parse::required_use(val, eapi)?,
            Restrict => self.restrict = dep::parse::restrict(val)?,
            SrcUri => self.src_uri = dep::parse::src_uri(val, eapi)?,
            Homepage => self.homepage = split(val).collect(),
            DefinedPhases => self.defined_phases = split(val).sorted().collect(),
            Keywords => self.keywords = split(val).collect(),
            Iuse => self.iuse = split(val).collect(),
            Inherit => self.inherit = split(val).collect(),
            Inherited => self.inherited = split(val).collect(),
            Eapi => (),
        }
        Ok(())
    }

    // TODO: use serde to support (de)serializing md5-cache metadata
    fn deserialize(s: &str, eapi: &'static Eapi) -> crate::Result<Self> {
        let mut meta = Metadata::default();

        let iter = s
            .lines()
            .filter_map(|l| {
                l.split_once('=').map(|(s, v)| match s {
                    "_eclasses_" => ("INHERITED", v),
                    _ => (s, v),
                })
            })
            .filter_map(|(k, v)| Key::from_str(k).ok().map(|k| (k, v)))
            .filter(|(k, _)| eapi.metadata_keys().contains(k));

        for (key, val) in iter {
            if key == Key::Inherited {
                meta.inherited = val
                    .split_whitespace()
                    .tuples()
                    .map(|(name, _chksum)| name.to_string())
                    .collect();
            } else {
                meta.convert(eapi, key, val)?;
            }
        }

        Ok(meta)
    }

    /// Load metadata from cache if available, otherwise source it from the ebuild content.
    pub(crate) fn load_or_source(
        cpv: &Cpv,
        data: &str,
        eapi: &'static Eapi,
        repo: &EbuildRepo,
    ) -> crate::Result<Self> {
        // TODO: compare ebuild mtime vs cache mtime
        match Self::load(cpv, eapi, repo) {
            Some(data) => Ok(data),
            None => Self::source(cpv, data, eapi, repo),
        }
    }

    /// Load metadata from cache.
    pub(crate) fn load(cpv: &Cpv, eapi: &'static Eapi, repo: &EbuildRepo) -> Option<Self> {
        // TODO: validate cache entries in some fashion?
        let path = build_from_paths!(repo.path(), "metadata", "md5-cache", cpv.to_string());
        let s = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    warn!("error loading ebuild metadata: {:?}: {e}", &path);
                }
                return None;
            }
        };

        match Metadata::deserialize(&s, eapi) {
            Ok(m) => Some(m),
            Err(e) => {
                warn!("error deserializing ebuild metadata: {:?}: {e}", &path);
                None
            }
        }
    }

    /// Source ebuild to determine metadata.
    pub(crate) fn source(
        cpv: &Cpv,
        data: &str,
        eapi: &'static Eapi,
        repo: &EbuildRepo,
    ) -> crate::Result<Self> {
        BuildData::update(cpv, repo, Some(eapi));
        // TODO: run sourcing via an external process pool returning the requested variables
        source_ebuild(data)?;
        let mut meta = Metadata::default();

        // verify sourced EAPI matches parsed EAPI
        let sourced_eapi: &Eapi = variables::optional("EAPI")
            .as_deref()
            .unwrap_or("0")
            .try_into()?;
        if sourced_eapi != eapi {
            return Err(Error::InvalidValue(format!(
                "mismatched sourced and parsed EAPIs: {sourced_eapi} != {eapi}"
            )));
        }

        // required metadata variables
        let mut missing = Vec::<&str>::new();
        for key in eapi.mandatory_keys() {
            match key.get(eapi) {
                Some(val) => meta.convert(eapi, *key, &val)?,
                None => missing.push(key.as_ref()),
            }
        }

        if !missing.is_empty() {
            missing.sort();
            let keys = missing.join(", ");
            return Err(Error::InvalidValue(format!("missing required values: {keys}")));
        }

        // metadata variables that default to empty
        for key in eapi.metadata_keys().difference(eapi.mandatory_keys()) {
            if let Some(val) = key.get(eapi) {
                meta.convert(eapi, *key, &val)?;
            }
        }

        // TODO: handle resets in external process pool
        scallop::shell::reset();

        Ok(meta)
    }

    pub(crate) fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn slot(&self) -> &str {
        let s = self.slot.as_str();
        s.split_once('/').map_or(s, |x| x.0)
    }

    pub(crate) fn subslot(&self) -> Option<&str> {
        let s = self.slot.as_str();
        s.split_once('/').map(|x| x.1)
    }

    pub(crate) fn deps(&self, key: Key) -> Option<&DepSet<Dep>> {
        self.deps.get(&key)
    }

    pub(crate) fn license(&self) -> Option<&DepSet<String>> {
        self.license.as_ref()
    }

    pub(crate) fn properties(&self) -> Option<&DepSet<String>> {
        self.properties.as_ref()
    }

    pub(crate) fn required_use(&self) -> Option<&DepSet<String>> {
        self.required_use.as_ref()
    }

    pub(crate) fn restrict(&self) -> Option<&DepSet<String>> {
        self.restrict.as_ref()
    }

    pub(crate) fn src_uri(&self) -> Option<&DepSet<Uri>> {
        self.src_uri.as_ref()
    }

    pub(crate) fn homepage(&self) -> &OrderedSet<String> {
        &self.homepage
    }

    pub(crate) fn defined_phases(&self) -> &OrderedSet<String> {
        &self.defined_phases
    }

    pub(crate) fn keywords(&self) -> &OrderedSet<String> {
        &self.keywords
    }

    pub(crate) fn iuse(&self) -> &OrderedSet<String> {
        &self.iuse
    }

    pub(crate) fn inherit(&self) -> &OrderedSet<String> {
        &self.inherit
    }

    pub(crate) fn inherited(&self) -> &OrderedSet<String> {
        &self.inherited
    }
}
