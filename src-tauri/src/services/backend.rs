use crate::services::assets::resolver::AssetResolver;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::director::WorldDirectorService;
use crate::services::game_engine::inventory::InventoryService;
use crate::services::game_engine::memory::MemoryService;
use crate::services::game_engine::orchestrator::SessionOrchestrator;
use crate::services::game_engine::rule::RuleEngineService;
use crate::services::game_engine::scene::SceneManager;
use crate::services::game_engine::state::StateEngineService;
use crate::services::game_engine::trigger::TriggerEngineService;
use crate::services::llm::LlmClient;
use std::path::PathBuf;

pub struct BackendServices {
    pub llm_client: LlmClient,
    pub runtime: RuntimeServices,
}

pub struct RuntimeServices {
    pub asset_resolver: AssetResolver,
    pub dialogue_pipeline: DialoguePipeline,
    pub inventory: InventoryService,
    pub memory: MemoryService,
    pub rule_engine: RuleEngineService,
    pub scene_manager: SceneManager,
    pub session_orchestrator: SessionOrchestrator,
    pub state_engine: StateEngineService,
    pub trigger_engine: TriggerEngineService,
    pub world_director: WorldDirectorService,
}

impl BackendServices {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            llm_client: LlmClient::new(),
            runtime: RuntimeServices::new(data_dir),
        }
    }
}

impl RuntimeServices {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            asset_resolver: AssetResolver::new(),
            dialogue_pipeline: DialoguePipeline::new(),
            inventory: InventoryService::new(),
            memory: MemoryService::with_data_dir(data_dir),
            rule_engine: RuleEngineService::new(),
            scene_manager: SceneManager::new(),
            session_orchestrator: SessionOrchestrator,
            state_engine: StateEngineService::new(),
            trigger_engine: TriggerEngineService::new(),
            world_director: WorldDirectorService::new(),
        }
    }
}
