// 裸 Lexical（无 wrapper）最小探针：判定是 wrapper 坏还是 Lexical 在这环境根本不工作。
import { describe, expect, it } from "vitest";
import { $createParagraphNode, $createTextNode, $getRoot, createEditor } from "lexical";

describe("bare lexical (webkit)", () => {
  it("createEditor -> setRootElement -> update -> textContent", async () => {
    const errors: string[] = [];
    const el = document.createElement("div");
    el.contentEditable = "true";
    document.body.appendChild(el);
    const editor = createEditor({
      namespace: "bare-probe",
      onError: (e: Error) => errors.push(String(e)),
    });
    editor.setRootElement(el);
    editor.update(
      () => {
        const root = $getRoot();
        const p = $createParagraphNode();
        p.append($createTextNode("xyz"));
        root.append(p);
      },
      { discrete: true },
    );
    const sync = editor.getEditorState().read(() => $getRoot().getTextContent());
    console.log("[bare] discrete sync=", JSON.stringify(sync), "errors=", errors);
    expect(sync).toBe("xyz");
    el.remove();
  });
});
