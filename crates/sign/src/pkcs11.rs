//! Digital IDs on PKCS #11 tokens (fork): the certificates a PKCS #11 module exposes, signing
//! with `C_Sign` so the private key never leaves the token. Made for the Certisign desktopID
//! cloud certificate (the module PJeOffice and Firefox load), and works with other modules too
//! (eToken, OpenSC).
//!
//! Tokens with a protected authentication path (desktopID) log in with no PIN: the module uses
//! the desktopID app's session or asks for approval on the phone. Other tokens take the PIN.
//! A login is kept for the whole run, so the token asks once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use cryptoki::context::{CInitializeArgs, Pkcs11};
use cryptoki::error::{Error as CkError, RvError};
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, AttributeType, ObjectClass};
use cryptoki::session::{Session, UserType};
use cryptoki::slot::Slot;
use cryptoki::types::AuthPin;

use crate::keys::{DigestAlg, ExternalKey, PrivateKey, PublicKey};
use crate::{Certificate, DigitalId, SignError};

/// Modules searched when `PDFCRAFT_PKCS11_MODULES` (paths separated by `:`) is not set: the ones
/// PJeOffice lists on this Mac, for this process's architecture.
fn default_modules() -> Vec<PathBuf> {
    let desktop_id = if cfg!(target_arch = "aarch64") {
        "/usr/local/lib/libdesktopID_Provider_ARM.dylib"
    } else {
        "/usr/local/lib/libdesktopID_Provider.dylib"
    };
    [desktop_id, "/usr/local/lib/libeTPkcs11.dylib"].iter().map(PathBuf::from).collect()
}

/// The modules to search, existing files only.
pub fn modules() -> Vec<PathBuf> {
    let listed = match std::env::var("PDFCRAFT_PKCS11_MODULES") {
        Ok(v) => v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect(),
        Err(_) => default_modules(),
    };
    listed.into_iter().filter(|p| p.is_file()).collect()
}

/// Each module is loaded and initialised once per process and never finalised: its sessions and
/// login live as long as PdfCraft.
fn context(module: &Path) -> Result<Pkcs11, SignError> {
    static LOADED: OnceLock<Mutex<HashMap<PathBuf, Pkcs11>>> = OnceLock::new();
    let mut loaded = LOADED.get_or_init(Default::default).lock().map_err(|_| SignError::Crypto("PKCS #11 lock poisoned".into()))?;
    if let Some(p) = loaded.get(module) {
        return Ok(p.clone());
    }
    let p = Pkcs11::new(module).map_err(|e| SignError::Crypto(format!("{}: {e}", module.display())))?;
    match p.initialize(CInitializeArgs::OsThreads) {
        Ok(()) | Err(CkError::AlreadyInitialized) | Err(CkError::Pkcs11(RvError::CryptokiAlreadyInitialized, _)) => {}
        Err(e) => return Err(SignError::Crypto(format!("{}: {e}", module.display()))),
    }
    loaded.insert(module.to_path_buf(), p.clone());
    Ok(p)
}

/// Where a token certificate's key lives.
#[derive(Clone)]
struct Location {
    module: PathBuf,
    slot: Slot,
    /// `CKA_ID` shared by the certificate and its private key.
    id: Vec<u8>,
    /// The token authenticates by itself (no PIN from PdfCraft).
    protected: bool,
    token: String,
}

/// The logged-in session of each token, by module and slot.
fn sessions() -> &'static Mutex<HashMap<(PathBuf, u64), Arc<Mutex<Session>>>> {
    static S: OnceLock<Mutex<HashMap<(PathBuf, u64), Arc<Mutex<Session>>>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// The token's session, logged in: an earlier one if there is one, or a new one (which may make
/// the token ask for approval, and take as long as that takes).
fn logged_in(loc: &Location, pin: Option<&str>) -> Result<Arc<Mutex<Session>>, SignError> {
    // One login at a time: a signature that comes while the early login waits for the phone
    // waits with it and then uses its session, instead of asking a second time.
    static LOGIN: Mutex<()> = Mutex::new(());
    let _one = LOGIN.lock().map_err(|_| SignError::Crypto("PKCS #11 lock poisoned".into()))?;
    let key = (loc.module.clone(), loc.slot.id());
    if let Some(s) = sessions().lock().map_err(|_| SignError::Crypto("PKCS #11 lock poisoned".into()))?.get(&key) {
        return Ok(s.clone());
    }
    let p = context(&loc.module)?;
    let session = p.open_ro_session(loc.slot).map_err(|e| SignError::Crypto(format!("{}: {e}", loc.token)))?;
    let pin = match (loc.protected, pin.filter(|p| !p.is_empty())) {
        (true, _) => None,
        (false, Some(p)) => Some(AuthPin::new(p.into())),
        (false, None) => return Err(SignError::WrongPassword),
    };
    match session.login(UserType::User, pin.as_ref()) {
        Ok(()) | Err(CkError::Pkcs11(RvError::UserAlreadyLoggedIn, _)) => {}
        Err(CkError::Pkcs11(RvError::PinIncorrect | RvError::PinLenRange | RvError::ArgumentsBad, _)) if !loc.protected => {
            return Err(SignError::WrongPassword);
        }
        Err(e) => return Err(SignError::Crypto(format!("{} didn't log in: {e}", loc.token))),
    }
    let session = Arc::new(Mutex::new(session));
    sessions().lock().map_err(|_| SignError::Crypto("PKCS #11 lock poisoned".into()))?.insert(key, session.clone());
    Ok(session)
}

struct TokenKey {
    loc: Location,
    pin: Option<String>,
}

impl ExternalKey for TokenKey {
    fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
        let mechanism = match alg {
            DigestAlg::Sha1 => Mechanism::Sha1RsaPkcs,
            DigestAlg::Sha384 => Mechanism::Sha384RsaPkcs,
            DigestAlg::Sha512 => Mechanism::Sha512RsaPkcs,
            _ => Mechanism::Sha256RsaPkcs,
        };
        let attempt = || -> Result<Vec<u8>, SignError> {
            let session = logged_in(&self.loc, self.pin.as_deref())?;
            let session = session.lock().map_err(|_| SignError::Crypto("PKCS #11 lock poisoned".into()))?;
            let key = session
                .find_objects(&[Attribute::Class(ObjectClass::PRIVATE_KEY), Attribute::Id(self.loc.id.clone())])
                .map_err(|e| SignError::Crypto(format!("{}: {e}", self.loc.token)))?
                .into_iter()
                .next()
                .ok_or_else(|| SignError::Crypto(format!("{} has no private key for this certificate", self.loc.token)))?;
            session.sign(&mechanism, key, msg).map_err(|e| SignError::Crypto(format!("{} didn't sign: {e}", self.loc.token)))
        };
        attempt().or_else(|e| {
            // A session the token dropped (desktopID's expires) is opened again, once.
            if let Ok(mut s) = sessions().lock() {
                s.remove(&(self.loc.module.clone(), self.loc.slot.id()));
            }
            attempt().map_err(|_| e)
        })
    }
}

/// The `pkcs11:<SHA-256 of the certificate>` reference PdfCraft keeps for a token identity.
pub fn reference(c: &Certificate) -> String {
    let d = DigestAlg::Sha256.digest(&[&c.raw]);
    format!("pkcs11:{}", d.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// The certificates of every token, with where their keys are. A module that doesn't load or a
/// slot that can't be read only means fewer identities.
fn listing() -> Vec<(Certificate, Location)> {
    let mut out = Vec::new();
    for module in modules() {
        let Ok(p) = context(&module) else { continue };
        let Ok(slots) = p.get_slots_with_token() else { continue };
        for slot in slots {
            let Ok(info) = p.get_token_info(slot) else { continue };
            let Ok(session) = p.open_ro_session(slot) else { continue };
            let Ok(certs) = session.find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE)]) else { continue };
            for handle in certs {
                let Ok(attrs) = session.get_attributes(handle, &[AttributeType::Id, AttributeType::Value]) else { continue };
                let (mut id, mut der) = (None, None);
                for a in attrs {
                    match a {
                        Attribute::Id(v) => id = Some(v),
                        Attribute::Value(v) => der = Some(v),
                        _ => {}
                    }
                }
                let (Some(id), Some(der)) = (id, der) else { continue };
                let Ok(certificate) = Certificate::parse(&der) else { continue };
                // Only RSA keys sign through the mechanisms used here, and only certificates
                // allowed to sign (key usage digitalSignature or nonRepudiation).
                if !matches!(certificate.public_key, PublicKey::Rsa { .. }) || certificate.key_usage.is_some_and(|u| u & 0b11 == 0) {
                    continue;
                }
                let token = info.label().trim().to_string();
                let loc = Location { module: module.clone(), slot, id, protected: info.protected_authentication_path(), token };
                out.push((certificate, loc));
            }
        }
    }
    out
}

/// A token identity as PdfCraft shows it: the certificate, its reference, the token's name and
/// whether it needs a PIN.
pub struct TokenId {
    pub certificate: Certificate,
    pub reference: String,
    pub token: String,
    pub needs_pin: bool,
}

/// Every signing certificate on the tokens of [`modules`].
pub fn identities() -> Vec<TokenId> {
    let mut out: Vec<TokenId> = Vec::new();
    for (certificate, loc) in listing() {
        let reference = reference(&certificate);
        if out.iter().any(|o| o.reference == reference) {
            continue;
        }
        out.push(TokenId { reference, token: loc.token.clone(), needs_pin: !loc.protected, certificate });
    }
    out
}

fn locate(reference: &str) -> Result<(Certificate, Location), SignError> {
    listing()
        .into_iter()
        .find(|(c, _)| self::reference(c) == reference)
        .ok_or_else(|| SignError::Crypto(format!("no token certificate {reference} (is the token connected?)")))
}

/// The identity a reference names, signing on its token. `pin` is for tokens without a
/// protected authentication path.
pub fn find(reference: &str, pin: Option<&str>) -> Result<DigitalId, SignError> {
    let (certificate, loc) = locate(reference)?;
    let name = format!("{} ({})", certificate.display_name(), loc.token);
    let key = PrivateKey::external(certificate.public_key.clone(), Arc::new(TokenKey { loc, pin: pin.map(str::to_string) }));
    Ok(DigitalId { key, certificate, chain: Vec::new(), friendly_name: Some(name) })
}

/// Log in to the token of `reference` ahead of signing, so a token that asks for approval asks
/// while the signature is being set up. Blocks until the token answers: call it off the UI thread.
pub fn prepare(reference: &str, pin: Option<&str>) -> Result<(), SignError> {
    let (_, loc) = locate(reference)?;
    logged_in(&loc, pin).map(|_| ())
}
