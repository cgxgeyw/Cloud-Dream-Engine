use crate::services::assets::resolver::AssetResolver;
use crate::services::game_engine::dialogue::DialoguePipeline;
use crate::services::game_engine::director::WorldDirectorService;
use crate::services::game_engine::memory::MemoryService;
use crate::services::game_engine::orchestrator::SessionOrchestrator;
use crate::services::game_engine::scene::SceneManager;
use crate::services::llm::LlmClient;
use std::path::PathBuf;

pub struct BackendServices {
    pub llm_client: LlmClient,
    pub runtime: RuntimeServices,
}

pub struct RuntimeServices {
    pub asset_resolver: AssetResolver,
    pub dialogue_pipeline: DialoguePipeline,
    pub memory: MemoryService,
    pub scene_manager: SceneManager,
    pub session_orchestrator: SessionOrchestrator,
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
            memory: MemoryService::with_data_dir(data_dir),
            scene_manager: SceneManager::new(),
            session_orchestrator: SessionOrchestrator,
            world_director: WorldDirectorService::new(),
        }
    }
}
