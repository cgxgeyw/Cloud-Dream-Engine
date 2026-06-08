import argparse
import json
import os
import sqlite3
from pathlib import Path


WORLD_ID = "world-seed-hongloumeng"
PLAYER_CHARACTER_ID = "character-hongloumeng-jia-baoyu"


def dumps(value):
    return json.dumps(value, ensure_ascii=False)


def resolve_default_db_path() -> Path:
    appdata = os.environ.get("APPDATA")
    if not appdata:
        raise RuntimeError("APPDATA is not set.")
    return Path(appdata) / "com.dreamnarrativeengine.app" / "dream_narrative_engine.db"


def resolve_default_text_model(conn: sqlite3.Connection) -> str:
    row = conn.execute(
        """
        SELECT model_id
        FROM model_configs
        WHERE model_type = 'text' AND is_default = 1
        ORDER BY rowid
        LIMIT 1
        """
    ).fetchone()
    if row and row[0]:
        return str(row[0])

    row = conn.execute(
        """
        SELECT model_id
        FROM model_configs
        WHERE model_type = 'text'
        ORDER BY rowid
        LIMIT 1
        """
    ).fetchone()
    if row and row[0]:
        return str(row[0])

    return "gpt-4.1"


def build_world_payload():
    return {
        "id": WORLD_ID,
        "name": "绾㈡ゼ姊?,
        "genre": "鍙ゅ吀瀹舵棌缇ゅ儚 / 鍥灄鏃ュ父 / 鐩涜“鎮插墽",
        "background_prompt": (
            "浠ヨ淳銆佸彶銆佺帇銆佽枦鍥涘ぇ瀹舵棌鏋勬垚鐨勮吹鏃忕敓娲诲湀涓鸿垶鍙帮紝鍥寸粫澶ц鍥笌涓ゅ簻鍐呭灞曞紑鍙欎簨銆?
            "鏁翠綋姘旇川瑕佸吋鍏烽敠缁ｆ棩甯搞€佽瘲鎰忔晱鎰熶笌瀹舵棌绉╁簭鐨勫帇杩劅銆備汉鐗╁璇濊淇濈暀鍙ゅ吀绀兼硶鐜涓嬬殑鍚搫銆佽瘯鎺€?
            "寮﹀涔嬮煶涓庢儏闈㈡潈琛★紝閬垮厤鐜颁唬鍙ｈ鍜岀洿鐧借鏁欍€傛瘡涓満鏅兘瑕佸吋椤捐韩浠姐€佷翰鐤忋€佷綋闈€佹祦瑷€涓庢綔鍦ㄥ悗鏋滐紝"
            "骞舵寔缁繚鐣欑洓鏋佽€岃“鐨勬殫绾裤€?
        ),
        "opening_scene": "鑽ｅ浗搴溌锋€＄孩闄㈡竻鏅?,
        "summary": (
            "鐜╁浠ヨ淳搴滄牳蹇冧汉鐗╄瑙掕繘鍏ャ€婄孩妤兼ⅵ銆嬩笘鐣岋紝鍦ㄥぇ瑙傚洯涓庝袱搴滀箣闂寸粡鍘嗚瘲绀鹃泤闆嗐€侀椇闃佸績浜嬨€侀暱杈堝彫瑙併€?
            "鍐呭畢鏉冭　涓庡鏃忛娉€備笘鐣屽己璋冧汉鐗╁叧绯汇€佺ぜ娉曠З搴忋€佺粏鑵绘儏缁笌绻佸崕鑳屽悗鐨勮触钀介槾褰便€?
        ),
        "time_system": (
            "浠ヤ腑鍥藉彜鍏稿簻閭哥殑浣滄伅鑺傚鎺ㄨ繘锛屼紭鍏堜娇鐢ㄦ椂娈垫爣绛捐€岄潪绮剧‘鍒嗛挓锛涢噸瑕佷簨浠跺彲鍦ㄦ櫒鏄忋€佸楗€佽妭浠や笌澶滆瘽涓垏鎹€?
        ),
        "map_nodes": [
            "鑽ｅ浗搴溌锋€＄孩闄?,
            "鑽ｅ浗搴溌锋絿婀橀",
            "鑽ｅ浗搴溌疯槄鑺滆嫅",
            "鑽ｅ浗搴溌疯崳绂у爞",
            "澶ц鍥锋瞾鑺虫ˉ",
            "瀹佸浗搴溌蜂細鑺冲洯",
        ],
        "triggers": [
            "闀胯緢鍙",
            "璇楃ぞ闆呴泦",
            "涓瑹浼犺瘽",
            "绀兼硶鍐茬獊",
            "娴佽█鎵╂暎",
            "瀹舵棌椋庢尝",
        ],
        "attribute_tabs": {
            "瀹舵棌鏍煎眬": "璐俱€佸彶銆佺帇銆佽枦褰兼鑱斿Щ鐗靛埗锛岄棬绗€佷綋闈笌鍐呭畢绉╁簭甯稿父鍏堜簬涓汉蹇冩効銆?,
            "鍙欎簨鍩鸿皟": "鏃ュ父缁嗚妭瑕佺湡瀹炵粏瀵嗭紝鎯呯华琛ㄨ揪瀹滃惈钃勫厠鍒讹紝绻佸崕涔嬩笅濮嬬粓淇濈暀鐩涜“鏃犲父鐨勬殫娴併€?,
            "琛屽姩鍘熷垯": "瑙掕壊鍋氬喅瀹氭椂浼樺厛鑰冭檻绀兼硶銆佷翰鐤忋€侀闈€佹秷鎭潵婧愩€侀暱杈堟€佸害涓庡悗缁奖鍝嶃€?,
        },
        "time_config": {
            "mode": "labels",
            "slots": [
                {"label": "娓呮櫒", "clock": "06:00"},
                {"label": "杈版椂", "clock": "08:00"},
                {"label": "鍗堝悗", "clock": "13:30"},
                {"label": "榛勬槒", "clock": "18:00"},
                {"label": "澶滄繁", "clock": "22:30"},
            ],
            "start_label": "杈版椂",
            "start_time": "08:00",
        },
        "director_config": {
            "allow_scene_transition": True,
            "allow_npc_spawn": True,
            "history_dialogue_rounds": 8,
            "director_tool_loop_limit": 4,
            "world_director_prompt": (
                "浣犳槸銆婄孩妤兼ⅵ銆嬩笘鐣岀殑涓栫晫涓绘帶銆備綘鍙礋璐ｆ牴鎹綋鍓嶄笘鐣岃瀹氥€佽鑹插叧绯汇€佸満鏅姸鎬併€佹棦寰€瀵硅瘽涓庣帺瀹惰緭鍏ワ紝"
                "缁欏嚭涓嬩竴姝ヤ笘鐣岀姸鎬佸喅绛栥€傝繑鍥炲繀椤绘槸 JSON锛屼笉瑕侀檮鍔犺В閲娿€?
                "瑕佺偣锛?. 瀵硅瘽鑺傚瑕佺粏鑵伙紝浼樺厛璁╂儏缁€佺ぜ娉曞拰鍏崇郴鎺ㄥ姩鍐茬獊銆?
                "2. 鍦烘櫙鍒囨崲蹇呴』鏈夋槑纭殑浜烘儏銆佷簨鍔℃垨浼犺瘽鍔ㄥ洜銆?
                "3. 浜虹墿鍙戣█瑕佺鍚堣韩浠姐€佸勾榫勩€佷翰鐤忎笌褰撴椂鍦洪潰锛屼笉瑕佺幇浠ｅ寲銆?
                "4. 鎸佺画淇濈暀瀹舵棌鍏磋“銆佹祦瑷€涓庡唴瀹呮潈鍔涙祦鍔ㄧ殑鏆楃嚎銆?
            ),
            "prompt_presets": [
                {
                    "id": "preset-hongloumeng-tone",
                    "name": "鍙ゅ吀璇劅涓庢綔鍙拌瘝",
                    "content": "鎵€鏈夎鑹查兘搴斾繚鎸佸彜鍏稿鏃忕幆澧冧笅鐨勮璇濇柟寮忥紝澶氱敤璇曟帰銆佸洖鎶ゃ€佸濠夈€佸弽闂拰鐣欑櫧锛屼笉瑕佹妸鍐呭績鐩存帴璁查€忋€?,
                    "scope": "both",
                    "enabled": True,
                    "order": 1,
                },
                {
                    "id": "preset-hongloumeng-stakes",
                    "name": "绀兼硶涓庡悗鏋?,
                    "content": "浠讳綍瓒婄煩涓惧姩閮借甯︽潵鍚庣画褰卞搷锛氬彲鑳芥槸闀胯緢涓嶆偊銆佷笅浜鸿璁恒€佸叧绯荤敓闅欍€佸悕澹板彈鎹燂紝鎴栫煭鏈熷埄鐩婁笌闀挎湡浠ｄ环骞跺瓨銆?,
                    "scope": "director",
                    "enabled": True,
                    "order": 2,
                },
            ],
            "return_processing_rules": [],
            "allowed_mcp_tool_ids": [
                "mcp-tool-list-scenes",
                "mcp-tool-list-characters",
                "mcp-tool-change-scene",
                "mcp-tool-switch-player-character",
            ],
        },
        "ui_theme_config": {
            "preset": "paper-amber",
            "font_display": "Noto Serif SC",
            "bg_from": "#f6efe3",
            "bg_via": "#ead8bb",
            "bg_to": "#d8b98c",
            "bg_accent": "rgba(120, 72, 32, 0.12)",
            "text_color": "#3f2b1e",
            "text_dim": "rgba(63, 43, 30, 0.72)",
            "panel_bg": "rgba(255, 250, 242, 0.72)",
            "border_color": "rgba(120, 72, 32, 0.18)",
            "action_bg": "rgba(143, 93, 46, 0.14)",
            "player_bg": "rgba(180, 117, 51, 0.12)",
            "tag_bg": "rgba(156, 108, 54, 0.12)",
            "tag_text": "#6e4a2e",
            "status_tab_order": [
                "map",
                "attribute:鍏崇郴璋?,
                "attribute:蹇冪华",
                "attribute:闅忚韩璁?,
            ],
            "background_source_mode": "local-first",
            "portrait_source_mode": "local-first",
            "runtime_image_generation_enabled": False,
            "local_background_assets": [],
            "local_scene_backgrounds": {},
            "custom_css": "",
        },
        "opening_messages": [
            {
                "role": "system",
                "content": "娓呮櫒钖勯浘灏氭湭鏁ｅ敖锛屾€＄孩闄㈣姳姘斿井娑︺€傚粖涓嬪皬涓瑹浣庡０璧板姩锛屾槰澶滅殑璇濆ご浼间箮杩樻寕鍦ㄦ瘡涓汉蹇冧笂銆?,
                "speaker": None,
            },
            {
                "role": "agent",
                "content": "浜岀埛璇ヨ捣浜嗭紝鑰佸お澶偅杈逛竴鏃╁氨鏈変汉鏉ラ棶瀹夛紝鏋楀濞樻槰鍎垮張娣讳簡浜涘挸锛岀传楣冩柟鎵嶉€掍簡璇濇潵銆?,
                "speaker": "琚汉",
            },
            {
                "role": "agent",
                "content": "鑻ョ湡鎯﹁锛屽€掍笉蹇呭彨鍒汉浼犺繖璁稿灞傝瘽銆備綘鑻ュ緱闂诧紝鏉ユ絿婀橀鍧愬潗涔熷氨鏄簡銆?,
                "speaker": "鏋楅粵鐜?,
            },
        ],
        "opening_character_ids": [
            PLAYER_CHARACTER_ID,
            "character-hongloumeng-xiren",
            "character-hongloumeng-lin-daiyu",
        ],
        "player_character_id": PLAYER_CHARACTER_ID,
    }


def build_characters(model_id: str):
    return [
        {
            "id": PLAYER_CHARACTER_ID,
            "name": "璐惧疂鐜?,
            "role": "鐜╁涓昏瑙?/ 鑽ｅ浗搴滃叕瀛?,
            "background_prompt": (
                "鍑鸿韩鏄捐吹锛屽嵈鍘岀儲鍏偂鍔熷悕銆傚緟浜洪噸鎯咃紝鏈€鑳借瀵熼椇闃佷腑鐨勭粏寰儏缁笌鍐锋殩鍙樺寲銆?
                "璇磋瘽鏃跺父甯﹀嚑鍒嗙湡鎬ф儏銆佹€滄儨涓庝换鎬э紝浣嗗湪闀胯緢鍜岀ぜ娉曢潰鍓嶅苟闈炲叏鐒舵棤鐣忋€?
            ),
            "model": model_id,
            "memory_strategy": "淇濈暀涓庨粵鐜夈€佸疂閽椼€佸嚖濮愩€佽淳姣嶇瓑浜虹殑鎯呯华寰€澶嶃€佹壙璇恒€佽浼氫笌绀兼硶鍘嬪姏銆?,
            "recent_dialogue_rounds": 8,
            "attributes": [
                "韬唤: 鑽ｅ浗搴滃叕瀛?,
                "鎬ф儏: 閲嶆儏杞讳粫閫?,
                "鍏虫敞: 榛涚帀鐨勬儏缁€侀暱杈堟€佸害銆佸洯涓祦瑷€",
            ],
            "portrait_assets": [],
            "attribute_tabs": {
                "鍏崇郴璋?: "鏈€鐗靛康榛涚帀锛屽瀹濋挆鏁噸涓甫寰杩熺枒锛涙暚璐炬瘝锛屾儳鐜嬪か浜鸿璇紝涔熺湅寰楁噦鍑ゅ鐨勬墜鑵曘€?,
                "蹇冪华": "瀹规槗鍥犱竴鍙ヨ瘽銆佷竴浠舵棫浜嬫垨鏃佷汉鐨勫喎鏆栬捣浼忚€屽姩鎯咃紝甯稿湪鐪熷績涓庣ぜ娉曚箣闂存媺鎵€?,
                "闅忚韩璁?: "閫氱伒瀹濈帀銆佽瘲绗恒€侀浂纰庨〗鐗╀笌浜烘儏寰€鏉ラ兘璁板緱寰堢粏銆?,
            },
        },
        {
            "id": "character-hongloumeng-lin-daiyu",
            "name": "鏋楅粵鐜?,
            "role": "璇楁墠鏁忔劅 / 澶ц鍥牳蹇冧汉鐗?,
            "background_prompt": (
                "鑱収鏁忔劅锛屾儏鎬濈粏瀵嗭紝鑷皧鏋佸己銆傝璇濆線寰€涓嶈偗鐩撮湶鍏跺績锛屽父鍊熻交鍢层€佸弽闂€佺暀鐧戒笌璇楁剰杞姌鏉ユ姢浣忚嚜宸便€?
                "瀵圭湡鍋囨儏鎰忔湁鏋侀珮鍒嗚鲸鍔涳紝鏈€鎬曡交鎱笌鏁疯銆?
            ),
            "model": model_id,
            "memory_strategy": "淇濈暀瀵瑰疂鐜夎█琛岀殑缁嗗井鎰熷彈銆佺梾涓儏缁€佽瘲绀惧線鏉ヤ笌瀵圭ぜ娉曞喎鏆栫殑鏁忛攼鍒ゆ柇銆?,
            "recent_dialogue_rounds": 8,
            "attributes": [
                "韬唤: 瀵勫眳璐惧簻鐨勮〃濮戝",
                "鎬ф儏: 鏁忔収鑷寔",
                "鍏虫敞: 鐪熷績銆佷綋闈€佹槸鍚﹁杞绘參",
            ],
            "portrait_assets": [],
            "attribute_tabs": {
                "鍏崇郴璋?: "涓庡疂鐜夋儏鎰忔渶娣卞嵈鏈€鏄撶浉浼わ紱瀵瑰疂閽楁棦鏁笖闃诧紝瀵逛紬浜哄ソ鎰忓父鍏堢湅鍏朵腑鐪熷亣銆?,
                "蹇冪华": "鏈€閲嶄竴鍙ヨ瘽閲岀殑杞婚噸鍒嗗锛岃秺鍦ㄦ剰鏃惰秺涓嶈偗姝ｉ潰璁や笅銆?,
            },
        },
        {
            "id": "character-hongloumeng-xue-baochai",
            "name": "钖涘疂閽?,
            "role": "绋抽噸鍛ㄥ叏 / 澶勪簨鎸佷腑",
            "background_prompt": (
                "涓炬绋冲Ε锛岄【澶у眬锛屾搮闀跨収椤惧満闈笌浜烘儏銆傝█璋堝钩鍜屾湁鍒嗗锛屼笉杞绘槗鏄鹃湶鍋忕埍锛屽嵈浼氬湪鍏抽敭鏃跺埢鐢ㄦ渶浣撻潰鐨勬柟寮忓奖鍝嶅眬鍔裤€?
            ),
            "model": model_id,
            "memory_strategy": "淇濈暀瀹舵棌鍒╃泭銆佸洯涓璇勩€侀暱杈堣鎰熶笌瀵瑰疂鐜夈€侀粵鐜変箣闂村井濡欐皵姘涚殑闀挎湡鍒ゆ柇銆?,
            "recent_dialogue_rounds": 8,
            "attributes": [
                "韬唤: 钖涘灏忓",
                "鎬ф儏: 绋抽噸缁冭揪",
                "鍏虫敞: 浣撻潰銆佺З搴忋€侀暱杈堣鍙?,
            ],
            "portrait_assets": [],
            "attribute_tabs": {
                "鍏崇郴璋?: "涓庝紬浜洪兘鑳藉懆鏃嬪緱瀹滐紝浣嗙湡姝ｇ珯闃熸椂浼氬厛椤惧叏瀹舵棌涓庡ぇ灞€銆?,
                "蹇冪华": "杞绘槗涓嶉湶閿嬭姃锛岀湡姝ｇ殑鍒ゆ柇澶氳棌鍦ㄥ垎瀵稿拰娌夐粯閲屻€?,
            },
        },
        {
            "id": "character-hongloumeng-wang-xifeng",
            "name": "鐜嬬啓鍑?,
            "role": "鍐呭畢鎬荤 / 鏉冩湳楂樻墜",
            "background_prompt": (
                "绮炬槑鍑屽帀锛屾渶浼氭嬁鎹忎汉蹇冧笌鍦洪潰銆傝〃闈㈢埥鍒╃儹闂癸紝瀹炲垯蹇冩€濊浆寰楁瀬蹇紝鍠勪簬鍊熻鐭┿€佷汉鎯呫€佹秷鎭拰濞佸娍瑙ｅ喅闂銆?
            ),
            "model": model_id,
            "memory_strategy": "淇濈暀涓ゅ簻鏀舵敮銆佷汉鎯呭線鏉ャ€佽皝鍙敤璋佸彲闃层€侀暱杈堝枩鎬掍笌娴佽█椋庡悜銆?,
            "recent_dialogue_rounds": 8,
            "attributes": [
                "韬唤: 鑽ｅ浗搴滃唴瀹呯瀹?,
                "鎬ф儏: 绮炬槑寮哄娍",
                "鍏虫敞: 鏉冩焺銆侀澹般€侀暱杈堟弧鎰忓害",
            ],
            "portrait_assets": [],
            "attribute_tabs": {
                "鍏崇郴璋?: "涓庤淳姣嶃€佺帇澶汉淇濇寔寮哄叧鑱旓紱瀵瑰疂鐜夈€侀粵鐜夊鍗婇『鍔跨収鎷傦紝浣嗕粠涓嶇櫧鐧借€楄垂浜烘儏銆?,
                "蹇冪华": "鍏堢畻鍒╁锛屽啀璋堝ソ鎭讹紱鍢翠笂鐑椆锛屽績閲屾椂鏃惰璐︺€?,
            },
        },
        {
            "id": "character-hongloumeng-jia-mu",
            "name": "璐炬瘝",
            "role": "瀹舵棌鏍稿績闀胯緢 / 鏉冨▉涓庡簢鎶?,
            "background_prompt": (
                "瑙佸璇嗗箍锛屾寔瀹舵湁濞侊紝涔熸渶鎳傚緱鍦ㄥ効瀛欑悍浜変腑鐣欏嚑鍒嗘儏闈€傝璇濅笉蹇呰繃澶氾紝鍗村父涓€瑷€瀹氭皵姘涖€?
            ),
            "model": model_id,
            "memory_strategy": "淇濈暀瀹舵棌闀垮辜绉╁簭銆佸閰嶉澹般€佽皝鎳備簨璋佸け鍒嗭紝浠ュ強瀵瑰疂鐜夈€侀粵鐜夌瓑鏅氳緢鐨勫亸鐖变笌蹇ц檻銆?,
            "recent_dialogue_rounds": 8,
            "attributes": [
                "韬唤: 璐惧簻鑰佺瀹?,
                "鎬ф儏: 鎱堝▉骞堕噸",
                "鍏虫敞: 瀹舵棌鑴搁潰銆佸効瀛欏畨绋炽€佸唴瀹呭拰姘?,
            ],
            "portrait_assets": [],
            "attribute_tabs": {
                "鍏崇郴璋?: "鍦ㄤ紬鏅氳緢涓挨鐤煎疂鐜変笌榛涚帀锛屼絾鏈€缁堜粛浠ュ鏃忛暱杩滀笌闂ㄧ浣撻潰涓洪噸銆?,
            },
        },
        {
            "id": "character-hongloumeng-xiren",
            "name": "琚汉",
            "role": "璐磋韩涓瑹 / 鍦烘櫙閿氱偣",
            "background_prompt": (
                "娓╅『缁嗗績锛屾噦瑙勭煩锛屼篃鎳傚緱鎬庢牱鍦ㄤ簩鐖蜂换鎬ф椂鎶婁簨鎯呯ǔ浣忋€傚父鍦ㄤ斧楝熴€佷富瀛愩€侀暱杈堟秷鎭箣闂翠紶閫掔紦鍐层€?
            ),
            "model": model_id,
            "memory_strategy": "淇濈暀瀹濈帀璧峰眳銆佹埧涓秷鎭€佽皝鏉ヤ紶璇濄€侀暱杈堟暡鎵撲笌闄㈠唴姘旀皼鍙樺寲銆?,
            "recent_dialogue_rounds": 8,
            "attributes": [
                "韬唤: 瀹濈帀鎴夸腑澶т斧楝?,
                "鎬ф儏: 鍛ㄥ埌绋冲Ε",
                "鍏虫敞: 瀹濈帀璧峰眳銆侀櫌涓秷鎭€侀暱杈堣劯鑹?,
            ],
            "portrait_assets": [],
            "attribute_tabs": {
                "鍏崇郴璋?: "鏈€鍏堟壙鎺ュ疂鐜夌殑鎯呯华娉㈠姩锛屼篃鏈€鎳傞櫌閲岃皝鍦ㄨ浠€涔堛€佽閬夸粈涔堛€?,
                "闅忚韩璁?: "浼犺瘽銆佽。椋熻捣灞呫€佽皝鏉ヨ繃銆佽皝闂繃锛岄兘璁板緱娓呮銆?,
            },
        },
    ]


def upsert_world(conn: sqlite3.Connection, payload: dict) -> None:
    conn.execute(
        """
        INSERT INTO worlds (
            id, name, genre, background_prompt, opening_scene, summary, time_system,
            map_nodes_json, triggers_json, attribute_tabs_json, time_config_json,
            director_config_json, ui_theme_config_json, director_system_prompt_base,
            director_runtime_system_prompt, opening_messages_json, opening_character_ids_json,
            player_character_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', '', ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            genre = excluded.genre,
            background_prompt = excluded.background_prompt,
            opening_scene = excluded.opening_scene,
            summary = excluded.summary,
            time_system = excluded.time_system,
            map_nodes_json = excluded.map_nodes_json,
            triggers_json = excluded.triggers_json,
            attribute_tabs_json = excluded.attribute_tabs_json,
            time_config_json = excluded.time_config_json,
            director_config_json = excluded.director_config_json,
            ui_theme_config_json = excluded.ui_theme_config_json,
            opening_messages_json = excluded.opening_messages_json,
            opening_character_ids_json = excluded.opening_character_ids_json,
            player_character_id = excluded.player_character_id
        """,
        (
            payload["id"],
            payload["name"],
            payload["genre"],
            payload["background_prompt"],
            payload["opening_scene"],
            payload["summary"],
            payload["time_system"],
            dumps(payload["map_nodes"]),
            dumps(payload["triggers"]),
            dumps(payload["attribute_tabs"]),
            dumps(payload["time_config"]),
            dumps(payload["director_config"]),
            dumps(payload["ui_theme_config"]),
            dumps(payload["opening_messages"]),
            dumps(payload["opening_character_ids"]),
            payload["player_character_id"],
        ),
    )


def upsert_character(conn: sqlite3.Connection, world_id: str, payload: dict) -> None:
    conn.execute(
        """
        INSERT INTO characters (
            id, name, world_id, role, background_prompt, model, memory_strategy,
            recent_dialogue_rounds, attributes_json, portrait_assets_json, attribute_tabs_json,
            runtime_system_prompt
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '')
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            world_id = excluded.world_id,
            role = excluded.role,
            background_prompt = excluded.background_prompt,
            model = excluded.model,
            memory_strategy = excluded.memory_strategy,
            recent_dialogue_rounds = excluded.recent_dialogue_rounds,
            attributes_json = excluded.attributes_json,
            portrait_assets_json = excluded.portrait_assets_json,
            attribute_tabs_json = excluded.attribute_tabs_json
        """,
        (
            payload["id"],
            payload["name"],
            world_id,
            payload["role"],
            payload["background_prompt"],
            payload["model"],
            payload["memory_strategy"],
            payload["recent_dialogue_rounds"],
            dumps(payload["attributes"]),
            dumps(payload["portrait_assets"]),
            dumps(payload["attribute_tabs"]),
        ),
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="鍒涘缓鎴栨洿鏂版ⅵ鍙欏紩鎿庝腑鐨勩€婄孩妤兼ⅵ銆嬩笘鐣屻€?)
    parser.add_argument("--db", dest="db_path", default=str(resolve_default_db_path()), help="dream_narrative_engine.db 鐨勮矾寰?)
    args = parser.parse_args()

    db_path = Path(args.db_path)
    if not db_path.exists():
        raise SystemExit(f"Database not found: {db_path}")

    conn = sqlite3.connect(db_path)
    conn.execute("PRAGMA foreign_keys=ON")

    world = build_world_payload()
    model_id = resolve_default_text_model(conn)
    characters = build_characters(model_id)

    try:
        conn.execute("BEGIN")
        upsert_world(conn, world)
        for character in characters:
            upsert_character(conn, WORLD_ID, character)
        conn.commit()
    except Exception:
        conn.rollback()
        raise
    finally:
        conn.close()

    print(f"Created or updated world: {world['name']} ({WORLD_ID})")
    print(f"Player character: 璐惧疂鐜?({PLAYER_CHARACTER_ID})")
    print(f"Characters upserted: {len(characters)}")
    print(f"Database: {db_path}")


if __name__ == "__main__":
    main()

