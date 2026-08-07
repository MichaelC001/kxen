// AttachMenu：+ 按钮（开启时旋转为 ×）+ 文件/图片选择。
// Tauri 走原生对话框拿真实绝对路径（附件授权与读取都要绝对路径，刻意不用 file input）；
// web 模式拿不到真实路径，改 <input type=file> 拿 File 对象走纯前端 inline（图片缩放 base64、文本 note）。
import { createSignal, Show } from "solid-js";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FilePlus2, ImagePlus, Plus } from "lucide-solid";
import { createExclusiveDisclosure, onClickOutside } from "../../lib/dismiss";
import { flashErr } from "../../lib/flash";
import { isWeb } from "../../lib/runtime";
import { errText } from "../err-text";

const IMAGE_EXTS = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const IMAGE_ACCEPT = IMAGE_EXTS.map((ext) => `.${ext}`).join(",");

export default function AttachMenu(props: {
  onPaths: (paths: string[]) => void;
  /** web 模式 file input 选中的 File 对象（Tauri 下不会触发）。 */
  onFiles?: (files: File[]) => void;
}) {
  const { open, setOpen, toggle } = createExclusiveDisclosure();
  const [acceptImages, setAcceptImages] = createSignal(false);
  let root: HTMLDivElement | undefined;
  let fileInput: HTMLInputElement | undefined;
  onClickOutside(
    () => root,
    () => setOpen(false),
  );

  const pickNative = async (images: boolean) => {
    setOpen(false);
    let selected: string | string[] | null;
    try {
      selected = await openDialog({
        multiple: true,
        title: images ? "选择图片" : "选择文件",
        ...(images ? { filters: [{ name: "图片", extensions: IMAGE_EXTS }] } : {}),
      });
    } catch (error) {
      flashErr(`打开附件选择器失败：${errText(error)}`);
      return;
    }
    const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
    if (paths.length > 0) props.onPaths(paths);
  };

  const pickWeb = (images: boolean) => {
    setOpen(false);
    setAcceptImages(images);
    fileInput?.click();
  };

  const pick = (images: boolean) => (isWeb() ? pickWeb(images) : void pickNative(images));

  return (
    <div class="relative" ref={(el) => (root = el)}>
      <button
        class="pressable action-icon attach-btn"
        classList={{ "attach-open": open() }}
        title="附件（选择文件或图片）"
        aria-expanded={open()}
        aria-haspopup="menu"
        onClick={toggle}
      >
        <Plus size={15} class="attach-icon" />
      </button>
      <Show when={open()}>
        <div class="composer-popup absolute bottom-full left-0 mb-1.5 w-44 max-w-[calc(100vw-16px)] rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] overflow-hidden z-20">
          <button class="popup-row" onClick={() => pick(true)}>
            <ImagePlus size={13} />
            选择图片
          </button>
          <button class="popup-row" onClick={() => pick(false)}>
            <FilePlus2 size={13} />
            选择文件
          </button>
        </div>
      </Show>
      {/* web 模式的文件入口：value 选中后清零，同名文件可连续再选 */}
      <input
        ref={(el) => (fileInput = el)}
        type="file"
        multiple
        class="hidden"
        accept={acceptImages() ? IMAGE_ACCEPT : undefined}
        onChange={(event) => {
          const files = [...(event.currentTarget.files ?? [])];
          event.currentTarget.value = "";
          if (files.length > 0) props.onFiles?.(files);
        }}
      />
    </div>
  );
}
