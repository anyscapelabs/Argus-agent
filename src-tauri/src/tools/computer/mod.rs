pub mod atspi;
pub mod x11;

use super::ToolMeta;

use crate::sessions::ext_install::data_dir;

pub fn shot_dir() -> std::path::PathBuf {
    data_dir().join("screenshots")
}

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "computer.observe",
        desc: "read the desktop as structured text: accessibility tree of every \
               window with element refs, roles, actions and positions, plus the \
               window list — the default way to see the screen",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.act",
        desc: "act on an element by ref from the last observe: press, toggle, \
               select — runs the app's own action when it has one, else clicks \
               the element's center; refusals include a fresh tree",
        args: "{\"ref\":5,\"action\":\"press\"}",
        mutating: true,
    },
    ToolMeta {
        name: "computer.screen",
        desc: "screenshot as an image — last resort, only when the tree cannot \
               show what you need (canvas-drawn apps); returns the image, its \
               size and the window list",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.click",
        desc: "synthetic click at x,y — screen coords from the latest computer.screen",
        args: "{\"x\":100,\"y\":200,\"button\":1,\"double\":false}",
        mutating: true,
    },
    ToolMeta {
        name: "computer.type",
        desc: "type text; give a ref to focus that field first (password fields \
               are refused — the user types those)",
        args: "{\"text\":\"...\",\"ref\":7}",
        mutating: true,
    },
    ToolMeta {
        name: "computer.key",
        desc: "press a key or combo like Return, ctrl+c, alt+Tab",
        args: "{\"key\":\"Return\"}",
        mutating: true,
    },
    ToolMeta {
        name: "computer.scroll",
        desc: "scroll up or down at x,y (defaults to the cursor)",
        args: "{\"x\":100,\"y\":200,\"dir\":\"down\",\"amount\":3}",
        mutating: true,
    },
    ToolMeta {
        name: "computer.window",
        desc: "activate or close a window by hex id from the observe list",
        args: "{\"action\":\"activate\",\"id\":\"0x03c00007\"}",
        mutating: true,
    },
];
