//! What the commands share: the database, the settings and the services built
//! from them.

use crate::config::{ConfigStore, Keychain, SecretStore};
use crate::db::Db;
use crate::error::Result;
use crate::integrations::camim::CamimDeliveryProvider;
use crate::integrations::inter::api::InterApi;
use crate::integrations::inter::slip::InterSlipProvider;
use crate::integrations::inter::statement::InterStatementProvider;
use crate::integrations::nfse::nacional::NfseNacionalProvider;
use crate::services::billing::BillingService;
use crate::services::delivery::{DeliveryProviders, DeliveryService};
use crate::services::import::{IbgeLookup, ImportService};
use crate::services::statement::StatementService;
use crate::util::statement_files::AccountInfo;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct Services {
    pub billing: Arc<BillingService>,
    pub delivery: DeliveryService,
    pub importer: ImportService,
    pub statements: StatementService,
}

pub struct AppState {
    pub db: Arc<Db>,
    pub config: ConfigStore,
    /// Where documents are written to be opened by other apps.
    pub cache_dir: PathBuf,
    // Built on first use and again after the settings change.
    services: Mutex<Option<Arc<Services>>>,
}

impl AppState {
    pub fn open(data_dir: &Path, cache_dir: &Path) -> Result<Self> {
        Self::with_secrets(data_dir, cache_dir, Box::new(Keychain))
    }

    pub fn with_secrets(
        data_dir: &Path,
        cache_dir: &Path,
        secrets: Box<dyn SecretStore>,
    ) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Self {
            db: Arc::new(Db::open(&data_dir.join("nf-fatura.db"))?),
            config: ConfigStore::new(data_dir.to_path_buf(), secrets),
            cache_dir: cache_dir.to_path_buf(),
            services: Mutex::new(None),
        })
    }

    pub fn services(&self) -> Result<Arc<Services>> {
        let mut cached = self.services.lock().unwrap();
        if let Some(services) = cached.as_ref() {
            return Ok(services.clone());
        }
        let env = self.config.env()?;
        let inter = Arc::new(InterApi::new(env.clone()));
        let nfse = Arc::new(NfseNacionalProvider::new(env.clone()));
        let slip = Arc::new(InterSlipProvider::new(inter.clone()));
        let billing = Arc::new(BillingService::new(
            self.db.clone(),
            nfse.clone(),
            slip.clone(),
        ));
        let mut providers: DeliveryProviders = Default::default();
        providers.insert(
            "CAMIM",
            Arc::new(CamimDeliveryProvider::new(
                &env.settings.camim_base_url,
                env.secrets.camim_token.clone(),
            )),
        );
        let services = Arc::new(Services {
            delivery: DeliveryService::new(self.db.clone(), providers, billing.clone()),
            importer: ImportService::new(self.db.clone(), nfse, slip, Arc::new(IbgeLookup)),
            statements: StatementService::new(
                Arc::new(InterStatementProvider::new(inter)),
                AccountInfo {
                    company_name: env.settings.company_name.clone(),
                    cnpj: env.settings.company_cnpj.clone().unwrap_or_default(),
                    branch: env.settings.inter_branch.clone(),
                    account: env.settings.inter_account.clone().unwrap_or_default(),
                },
            ),
            billing,
        });
        *cached = Some(services.clone());
        Ok(services)
    }

    /// The next use rebuilds the services from the current settings.
    pub fn reset_services(&self) {
        *self.services.lock().unwrap() = None;
    }
}
