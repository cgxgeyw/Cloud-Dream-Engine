import { useEffect, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  createSession,
  fetchWorld,
  fetchWorldCharacters,
  fetchWorlds,
  type CharacterResponse,
  type WorldResponse,
} from "../data/apiAdapter";
import { ScreenLayout, SurfacePanel } from "../components/ScreenLayout";
import { ArrowLeft } from "lucide-react";

type PageProps = {
  isMobile?: boolean;
};

function countMapNodes(value: WorldResponse["map_nodes"]): number {
  let count = 0;
  function visit(node: unknown) {
    if (!node || typeof node !== "object" || Array.isArray(node)) {
      return;
    }
    count += 1;
    const children = (node as { children?: unknown }).children;
    if (Array.isArray(children)) {
      children.forEach(visit);
    }
  }

  const root = value.root ?? value.tree;
  if (root) {
    visit(root);
  } else if (Array.isArray(value.nodes)) {
    value.nodes.forEach(visit);
  }
  return count;
}

function useWorldList(preferredWorldId?: string | null) {
  const [worlds, setWorlds] = useState<WorldResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadWorlds() {
      try {
        setLoading(true);
        setError(null);
        const worldData = await fetchWorlds();
        if (!cancelled) {
          const sorted = preferredWorldId
            ? [...worldData].sort((left, right) => {
                if (left.id === preferredWorldId) return -1;
                if (right.id === preferredWorldId) return 1;
                return 0;
              })
            : worldData;
          setWorlds(sorted);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "加载世界列表失败");
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadWorlds();
    return () => {
      cancelled = true;
    };
  }, [preferredWorldId]);

  return { worlds, loading, error };
}

export function NewGamePage(_props: PageProps = {}) {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const preferredWorldId = searchParams.get("worldId");
  const { worlds, loading, error } = useWorldList(preferredWorldId);

  return (
    <ScreenLayout
      title="新游戏"
      subtitle="选择一个世界，进入开局设定。"
      compactHeader
      maxWidth={980}
      toolbar={
        <button type="button" onClick={() => navigate(-1)} className="action-btn">
          <ArrowLeft size={14} /> 首页
        </button>
      }
    >
      {loading ? <div className="empty-text">正在加载世界列表...</div> : null}
      {error ? <div className="error-text">加载失败：{error}</div> : null}

      {!loading && !error && worlds.length === 0 ? (
        <div className="empty-text">暂无世界，请先创建一个世界。</div>
      ) : null}

      {!loading && !error && worlds.length > 0 ? (
        <div className="newgame-screen">
          <SurfacePanel className="surface-panel--pad-lg newgame-list-panel">
            <div className="newgame-list-head">
              <div>
                <strong className="newgame-section-title">选择世界</strong>
              </div>
              <button type="button" className="action-btn" onClick={() => navigate("/worlds")}>
                管理世界
              </button>
            </div>

            <div className="newgame-world-grid">
              {worlds.map((world) => (
                <button
                  key={world.id}
                  type="button"
                  className={`newgame-world-card${world.id === preferredWorldId ? " newgame-world-card--active" : ""}`}
                  onClick={() => navigate(`/new-game/setup/${world.id}`)}
                >
                  <div className="newgame-world-card-top">
                    <strong>{world.name}</strong>
                    <span>{world.genre || "未分类"}</span>
                  </div>
                  <p>{world.summary || "这个世界还没有简介。"}</p>
                  <div className="newgame-world-card-meta">
                    <span>开场：{world.opening_scene || "未设置"}</span>
                    <span>地图 {countMapNodes(world.map_nodes)}</span>
                  </div>
                </button>
              ))}
            </div>
          </SurfacePanel>
        </div>
      ) : null}
    </ScreenLayout>
  );
}

export function NewGameSetupPage(_props: PageProps = {}) {
  const navigate = useNavigate();
  const { worldId = "" } = useParams();
  const [world, setWorld] = useState<WorldResponse | null>(null);
  const [characters, setCharacters] = useState<CharacterResponse[]>([]);
  const [selectedPlayerCharacterId, setSelectedPlayerCharacterId] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function loadSetupData() {
      try {
        setLoading(true);
        setError(null);
        const [worldData, characterData] = await Promise.all([
          fetchWorld(worldId),
          fetchWorldCharacters(worldId),
        ]);
        if (!cancelled) {
          setWorld(worldData);
          setCharacters(characterData);
          setSelectedPlayerCharacterId(worldData.player_character_id ?? characterData[0]?.id ?? "");
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "加载开局设定失败");
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    if (worldId) {
      void loadSetupData();
    } else {
      setLoading(false);
      setError("缺少世界 ID");
    }

    return () => {
      cancelled = true;
    };
  }, [worldId]);

  async function handleCreateSession() {
    if (!world) {
      return;
    }
    if (!selectedPlayerCharacterId) {
      setError("请先选择玩家角色");
      return;
    }

    try {
      setCreating(true);
      setError(null);
      const session = await createSession({
        world_id: world.id,
        player_character_id: selectedPlayerCharacterId,
      });
      navigate(`/game/${session.id}`);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "启动会话失败");
    } finally {
      setCreating(false);
    }
  }

  return (
    <ScreenLayout
      title="开局设定"
      subtitle="确认开场信息，并选择这次由你操控的角色。"
      compactHeader
      maxWidth={860}
      toolbar={
        <button type="button" onClick={() => navigate(-1)} className="action-btn">
          <ArrowLeft size={14} /> 返回
        </button>
      }
    >
      {loading ? <div className="empty-text">正在加载开局数据...</div> : null}
      {error ? <div className="error-text">加载失败：{error}</div> : null}

      {!loading && !error && world ? (
        <div className="newgame-screen newgame-setup-screen">
          <div className="newgame-setup-panel">


            <div className="newgame-preview-hero">
              <div className="newgame-label">即将开始</div>
              <h2 className="newgame-world-name">{world.name}</h2>
              <div className="newgame-world-meta">{world.genre || "未分类世界"}</div>
              <p className="newgame-preview-summary">{world.summary || "这个世界还没有简介。"}</p>
            </div>

            <div className="newgame-info-grid">
              <div className="newgame-info-row">
                <strong>开场地点</strong>
                <span>{world.opening_scene || "未设置"}</span>
              </div>
              <div className="newgame-info-row">
                <strong>时间系统</strong>
                <span>{world.time_system || "未设置"}</span>
              </div>
              <div className="newgame-info-row newgame-info-row--wide">
                <strong>在场角色</strong>
                <span>{characters.length > 0 ? characters.map((item) => item.name).join(" / ") : "暂无角色"}</span>
              </div>
            </div>

            <div className="newgame-player-card">
              <label className="editor-field">
                <span className="editor-field-label">选择你的角色</span>
                <select
                  value={selectedPlayerCharacterId}
                  onChange={(event) => setSelectedPlayerCharacterId(event.target.value)}
                  className="editor-field-input editor-field-select"
                >
                  {characters.map((character) => (
                    <option key={character.id} value={character.id}>
                      {character.name}
                      {world.player_character_id === character.id ? "（默认）" : ""}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div className="newgame-action-stack">
              <div className="newgame-secondary-actions">
                <button type="button" className="action-btn" onClick={() => navigate(`/worlds/${world.id}/edit`)}>
                  编辑世界
                </button>
                <button
                  type="button"
                  onClick={() => navigate(`/worlds/${world.id}/characters`)}
                  className="action-btn"
                >
                  角色池
                </button>
              </div>
              <button
                type="button"
                onClick={() => void handleCreateSession()}
                disabled={creating || !selectedPlayerCharacterId}
                className="action-btn action-btn--primary newgame-start-btn"
              >
                {creating ? "启动中..." : "进入世界"}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </ScreenLayout>
  );
}
