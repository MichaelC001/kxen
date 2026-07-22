// AttachMenu：+ 按钮（开启时旋转为 ×）+ 文件/图片选择（hidden input）。
import { createSignal, Show } from "solid-js";
import { FilePlus2, ImagePlus, Plus } from "lucide-solid";

export default function AttachMenu(props: { onFiles: (files: FileList) => void }) {
  const [open, setOpen] = createSignal(false);
  let fileInput: HTMLInputElement | undefined;
  let imageInput: HTMLInputElement | undefined;

  const pick = (input: HTMLInputElement | undefined) => {
    setOpen(false);
    input?.click();
  };

  return (
    <div class="relative">
      <button
        class="pressable action-icon attach-btn"
        classList={{ "attach-open": open() }}
        title="附件（选择文件或图片）"
        onClick={() => setOpen(!open())}
      >
        <Plus size={15} class="attach-icon" />
      </button>
      <Show when={open()}>
        <div class="composer-popup absolute bottom-full left-0 mb-1.5 w-44 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
          <button class="popup-row" onClick={() => pick(imageInput)}>
            <ImagePlus size={13} />
            选择图片
          </button>
          <button class="popup-row" onClick={() => pick(fileInput)}>
            <FilePlus2 size={13} />
            选择文件
          </button>
        </div>
      </Show>
      <input
        ref={(el) => (imageInput = el)}
        type="file"
        accept="image/*"
        multiple
        class="hidden"
        onChange={(e) => e.currentTarget.files && props.onFiles(e.currentTarget.files)}
      />
      <input
        ref={(el) => (fileInput = el)}
        type="file"
        multiple
        class="hidden"
        onChange={(e) => e.currentTarget.files && props.onFiles(e.currentTarget.files)}
      />
    </div>
  );
}
