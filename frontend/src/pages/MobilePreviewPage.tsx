import type { ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { ScreenLayout } from "../components/ScreenLayout";
import { Menu, X } from "lucide-react";

type MobileMockProps = {
  label: string;
  note: string;
  children: ReactNode;
};

const drawerItems = [
  { title: "新的游戏", note: "快速开局与继续会话", active: true },
  { title: "读取存档", note: "回到旧时间线或分支" },
  { title: "世界设计", note: "编辑世界、场景与提示词" },
  { title: "设置", note: "模型、导出、调试显示" },
  { title: "MCP 工具管理", note: "管理世界主控可调用工具" },
];

const worlds = [
  { name: "红楼梦世界", meta: "荣国府 · 12 名在场角色", tone: "rose" },
  { name: "金陵夜雨", meta: "秦可卿支线 · 第 8 回合", tone: "ink" },
  { name: "大观园宴游", meta: "多人场景 · 流式对话", tone: "gold" },
];

const messages = [
  { speaker: "旁白", tone: "ambient", content: "晨雾还未散尽，怡红院外竹影轻晃，丫鬟们都压低了脚步。" },
  { speaker: "林黛玉", tone: "character", content: "今儿风凉，我原想去看你，却又怕惊了院里的清静。" },
  { speaker: "贾宝玉", tone: "player", content: "那便进来坐，我叫袭人把新温的茶送上来。" },
  { speaker: "世界主控", tone: "system", content: "场景已转入近距离私谈，适合触发心绪、回忆与细节观察。" },
];

const settingsItems = [
  { label: "文本模型", value: "DeepSeek V3 · 流式" },
  { label: "Embedding", value: "bge-small-zh-v1.5 · 已启用" },
  { label: "导出目录", value: "手机沙盒 / 导出 / 红楼梦" },
  { label: "调试显示", value: "显示思维链与工具调用" },
];

const editorCards = [
  { title: "世界概览", value: "红楼梦世界", note: "一句简介、时代气息、叙事基调" },
  { title: "角色编排", value: "12 人", note: "谁可登场、谁可切换、谁只旁观" },
  { title: "场景资产", value: "27 项", note: "背景图、立绘、局部音效、场景标签" },
  { title: "提示词", value: "4 组", note: "世界主控、角色、旁白、记忆召回" },
];

function MobileMock({ label, note, children }: MobileMockProps) {
  return (
    <section className="mobile-redesign-card">
      <div className="mobile-redesign-caption">
        <strong>{label}</strong>
        <span>{note}</span>
      </div>
      <div className="mobile-redesign-phone">
        <div className="mobile-redesign-status">
          <span>9:41</span>
          <span>5G</span>
          <span>92%</span>
        </div>
        <div className="mobile-redesign-screen">{children}</div>
      </div>
    </section>
  );
}

export function MobilePreviewPage() {
  const navigate = useNavigate();

  return (
    <ScreenLayout
      title="手机端界面预览"
      subtitle="这次完全推倒重做，不再使用统一页头。所有界面改成浮动按钮 + 内容直接入场。"
      maxWidth={1400}
      toolbar={
        <button type="button" onClick={() => navigate("/")} className="action-btn">
          返回首页
        </button>
      }
    >
      <div className="mobile-redesign-hero">
        <div className="mobile-redesign-hero-copy">
          <span className="mobile-redesign-kicker">New Direction</span>
          <h2>去掉每个界面顶部那条统一页头，改成更像 App 的浮动控制和直接入场内容。</h2>
          <p>
            左侧导航从最顶层直接盖出，游戏页不再切出单独场景区，状态区也不再做成明显的第二栏，
            而是缩成右侧轻抽屉。整套界面优先消掉“网页分区感”。
          </p>
        </div>
        <div className="mobile-redesign-principles">
          <div className="mobile-redesign-principle">
            <strong>无统一页头</strong>
            <span>每个页面顶部不再有同构导航条，只保留小型浮动按钮和必要信息点。</span>
          </div>
          <div className="mobile-redesign-principle">
            <strong>左抽屉优先</strong>
            <span>电脑版首页入口都收进左抽屉，抽屉直接从最顶层滑出，盖住当前内容。</span>
          </div>
          <div className="mobile-redesign-principle">
            <strong>聊天优先</strong>
            <span>游戏页一切围绕聊天主线组织，背景、立绘、状态只做陪衬，不再切主舞台。</span>
          </div>
        </div>
      </div>

      <div className="mobile-redesign-grid">
        <MobileMock label="首页 / 左抽屉" note="无页头，左上角小按钮呼出整屏抽屉">
          <div className="mobile-redesign-scene mobile-redesign-scene--home">
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--left" aria-label="打开导航">
              <Menu size={16} />
            </button>
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--right" aria-label="更多">
              ···
            </button>

            <div className="mobile-redesign-home-content mobile-redesign-home-content--dimmed">
              <div className="mobile-redesign-banner">
                <span className="mobile-redesign-chip">继续推进</span>
                <strong>怡红院晨起局</strong>
                <p>从上次停下来的第 34 回合继续，直接进入对话。</p>
                <button type="button" className="mobile-redesign-primary-btn">
                  继续上次会话
                </button>
              </div>

              <div className="mobile-redesign-section-title">最近世界</div>
              <div className="mobile-redesign-world-list">
                {worlds.map((item) => (
                  <article key={item.name} className={`mobile-redesign-world mobile-redesign-world--${item.tone}`}>
                    <strong>{item.name}</strong>
                    <span>{item.meta}</span>
                  </article>
                ))}
              </div>
            </div>

            <aside className="mobile-redesign-drawer">
              <div className="mobile-redesign-drawer-head">
                <div />
                <button type="button" className="mobile-redesign-ghost-btn" aria-label="关闭导航">
                  <X size={14} />
                </button>
              </div>
              <div className="mobile-redesign-drawer-list">
                {drawerItems.map((item) => (
                  <article
                    key={item.title}
                    className={`mobile-redesign-drawer-item${item.active ? " mobile-redesign-drawer-item--active" : ""}`}
                  >
                    <strong>{item.title}</strong>
                    <span>{item.note}</span>
                  </article>
                ))}
              </div>
            </aside>
          </div>
        </MobileMock>

        <MobileMock label="游戏页" note="聊天背景化，顶部只留极轻信息点，状态从右侧推出">
          <div className="mobile-redesign-scene mobile-redesign-scene--game">
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--left" aria-label="打开导航">
              <Menu size={16} />
            </button>
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--right" aria-label="更多">
              ···
            </button>

            <div className="mobile-redesign-game-bg">
              <div className="mobile-redesign-game-fade" />
              <div className="mobile-redesign-portrait">黛玉</div>
            </div>

            <div className="mobile-redesign-game-strip">
              <span>荣国府 · 怡红院清晨</span>
              <span>第 34 回合</span>
              <span>在场 6 人</span>
            </div>

            <div className="mobile-redesign-chat">
              {messages.map((message) => (
                <article
                  key={`${message.speaker}-${message.content}`}
                  className={`mobile-redesign-bubble mobile-redesign-bubble--${message.tone}`}
                >
                  <div className="mobile-redesign-bubble-speaker">{message.speaker}</div>
                  <div className="mobile-redesign-bubble-content">{message.content}</div>
                  {message.tone !== "ambient" ? (
                    <div className="mobile-redesign-bubble-thought">
                      思维链预览：先判断情绪，再结合最近两回合与命中记忆，最后生成措辞。
                    </div>
                  ) : null}
                </article>
              ))}
            </div>

            <div className="mobile-redesign-status-handle">状态</div>
            <aside className="mobile-redesign-status-drawer">
              <div className="mobile-redesign-status-item">
                <strong>当前主角</strong>
                <span>贾宝玉 · 注意力集中在黛玉</span>
              </div>
              <div className="mobile-redesign-status-item">
                <strong>背包与物件</strong>
                <span>通灵宝玉、旧诗稿、温茶一盏</span>
              </div>
              <div className="mobile-redesign-status-item">
                <strong>世界标签</strong>
                <span>私谈、清晨、轻微试探、回忆触发</span>
              </div>
            </aside>

            <div className="mobile-redesign-input">
              <button type="button" className="mobile-redesign-plus-btn" aria-label="附加操作">
                ＋
              </button>
              <div className="mobile-redesign-input-box">想说点什么？这里支持旁白式输入、动作式输入和重发。</div>
              <button type="button" className="mobile-redesign-send-btn">
                发送
              </button>
            </div>
          </div>
        </MobileMock>

        <MobileMock label="世界编辑" note="入口从左抽屉进入，内部改成无页头分页卡片">
          <div className="mobile-redesign-scene mobile-redesign-scene--editor">
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--left" aria-label="打开导航">
              <Menu size={16} />
            </button>
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--right" aria-label="更多">
              ···
            </button>

            <div className="mobile-redesign-page-title">
              <strong>红楼梦世界</strong>
              <span>长篇群像 · 高密度社交场景 · 世界主控启用</span>
            </div>

            <div className="mobile-redesign-segments">
              <span className="mobile-redesign-segments-item mobile-redesign-segments-item--active">概览</span>
              <span className="mobile-redesign-segments-item">角色</span>
              <span className="mobile-redesign-segments-item">场景</span>
              <span className="mobile-redesign-segments-item">提示词</span>
            </div>

            <div className="mobile-redesign-editor-list">
              {editorCards.map((item) => (
                <article key={item.title} className="mobile-redesign-editor-card">
                  <div>
                    <strong>{item.title}</strong>
                    <span>{item.note}</span>
                  </div>
                  <em>{item.value}</em>
                </article>
              ))}
            </div>

            <div className="mobile-redesign-bottom-action">
              <button type="button" className="mobile-redesign-primary-btn">
                选择图片
              </button>
              <button type="button" className="mobile-redesign-secondary-btn">
                保存
              </button>
            </div>
          </div>
        </MobileMock>

        <MobileMock label="设置" note="不再做设置页头，直接从列表开始">
          <div className="mobile-redesign-scene mobile-redesign-scene--settings">
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--left" aria-label="打开导航">
              <Menu size={16} />
            </button>
            <button type="button" className="mobile-redesign-fab mobile-redesign-fab--right" aria-label="更多">
              ···
            </button>

            <div className="mobile-redesign-settings-title">
              <strong>模型与运行设置</strong>
              <span>入口统一放在左抽屉，当前页只负责配置内容本身。</span>
            </div>

            <div className="mobile-redesign-settings-list">
              {settingsItems.map((item) => (
                <article key={item.label} className="mobile-redesign-settings-row">
                  <div>
                    <strong>{item.label}</strong>
                    <span>{item.value}</span>
                  </div>
                  <em>›</em>
                </article>
              ))}
            </div>

            <div className="mobile-redesign-toggle-card">
              <div>
                <strong>流式输出</strong>
                <span>角色正文、思维链、旁白都按流式逐步出现。</span>
              </div>
              <div className="mobile-redesign-toggle mobile-redesign-toggle--on">
                <span />
              </div>
            </div>
          </div>
        </MobileMock>
      </div>
    </ScreenLayout>
  );
}
