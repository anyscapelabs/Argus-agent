pub mod x11;

use super::ToolMeta;

// Gating lands with the Connectors card — every action is open for now.
pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "computer.observe",
        desc: "screenshot the desktop; returns the image, its size and the window list",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.click",
        desc: "click at x,y from the latest screenshot; returns a fresh screenshot",
        args: "{\"x\":100,\"y\":200,\"button\":1,\"double\":false}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.type",
        desc: "type text into the focused field; returns a fresh screenshot",
        args: "{\"text\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.key",
        desc: "press a key or combo like Return, ctrl+c, alt+Tab; returns a fresh screenshot",
        args: "{\"key\":\"Return\"}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.scroll",
        desc: "scroll up or down at x,y (defaults to the cursor); returns a fresh screenshot",
        args: "{\"x\":100,\"y\":200,\"dir\":\"down\",\"amount\":3}",
        mutating: false,
    },
    ToolMeta {
        name: "computer.window",
        desc: "activate or close a window by hex id from the observe list",
        args: "{\"action\":\"activate\",\"id\":\"0x03c00007\"}",
        mutating: false,
    },
];
