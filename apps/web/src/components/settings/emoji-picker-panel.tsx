import EmojiPicker, { EmojiStyle, Theme, type EmojiClickData } from "emoji-picker-react";

// The picker bundles all emoji data. This module is the ONLY place that imports
// emoji-picker-react, and it is reached exclusively via `lazy()` in
// emoji-field.tsx — so the whole library lands in its own async chunk instead
// of the initial settings bundle. Importing the `EmojiStyle`/`Theme` enums
// (runtime values) from a statically-loaded module would undo that.
export default function EmojiPickerPanel({
  isDark,
  onPick,
}: {
  isDark: boolean;
  onPick: (data: EmojiClickData) => void;
}) {
  return (
    <EmojiPicker
      onEmojiClick={onPick}
      emojiStyle={EmojiStyle.NATIVE}
      theme={isDark ? Theme.DARK : Theme.LIGHT}
      lazyLoadEmojis
      previewConfig={{ showPreview: false }}
      autoFocusSearch={false}
      height={350}
      width={320}
    />
  );
}
