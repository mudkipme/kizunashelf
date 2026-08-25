//! The editor for `image` / `imageList` fields.
//!
//! Images can arrive three ways — a vault-relative path, a remote URL, or bytes
//! dropped/picked from the host — and all three end up as the same frontmatter
//! value. Uploading stages the returned vault path into the draft; nothing is
//! written until the normal Save.

import type { DragEvent } from "react";
import { useRef, useState } from "react";
import { useLingui } from "@lingui/react/macro";
import { ImageIcon, Loader2Icon, UploadIcon, XIcon } from "lucide-react";
import { toast } from "sonner";

import { uploadAsset } from "@/api/entities";
import { AssetImage } from "@/components/assets/asset-image";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

import { listDisplayValues, uniqueStrings, valueToText } from "./frontmatter-utils";
import { MultiValueInput } from "./metadata-list-inputs";
import type { EditableFieldSpec, FrontmatterValue } from "./metadata-types";

const IMAGE_ACCEPT = "image/*";
const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp|avif|bmp|svg|tiff?)$/i;

function isImageFile(file: File): boolean {
  return file.type.startsWith("image/") || IMAGE_EXTENSION.test(file.name);
}

/** Encode file bytes as base64 for the upload endpoint, chunked to keep large
 * images off the call stack (`String.fromCharCode(...bytes)` overflows). */
async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  const chunk = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunk) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunk));
  }
  return btoa(binary);
}

/**
 * Editor for `image` / `imageList` fields: shows thumbnails of the current
 * value(s), keeps the raw path/URL entry (so vault assets and remote URLs still
 * work), and — for an existing entity with writes enabled — adds a file picker /
 * drag-and-drop that uploads bytes into the vault and stages the returned path
 * into the draft (persisted on the normal Save).
 */
export function ImageFieldInput({
  field,
  value,
  disabled,
  entityId,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
  onChange: (value: FrontmatterValue) => void;
}) {
  const { t } = useLingui();
  const multiple = field.kind === "imageList";
  const values = listDisplayValues(value, false);
  const single = multiple ? "" : valueToText(value);
  const [uploading, setUploading] = useState(false);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const canUpload = Boolean(entityId) && !disabled;

  async function uploadFiles(files: File[]) {
    if (!entityId || uploading) return;
    const images = files.filter(isImageFile);
    if (images.length === 0) {
      if (files.length > 0) toast.error(t`Only image files can be uploaded.`);
      return;
    }
    setUploading(true);
    try {
      const added: string[] = [];
      for (const file of multiple ? images : images.slice(0, 1)) {
        const result = await uploadAsset(entityId, {
          field: field.key,
          dataBase64: await fileToBase64(file),
          contentType: file.type || undefined,
          filename: file.name || undefined,
        });
        added.push(result.path);
      }
      if (multiple) onChange(uniqueStrings([...values, ...added]));
      else if (added[0]) onChange(added[0]);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t`Upload failed.`);
    } finally {
      setUploading(false);
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    if (canUpload) void uploadFiles(Array.from(event.dataTransfer.files));
  }

  return (
    <div className="flex flex-col gap-2">
      {multiple
        ? values.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {values.map((item, index) => (
                <div key={`${item}-${index}`} className="group relative size-16 overflow-hidden rounded-md border">
                  <AssetImage src={item} alt="" className="size-full object-cover" fallback={<ImageThumbFallback />} lightbox />
                  {!disabled ? (
                    <button
                      type="button"
                      onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
                      aria-label={t`Remove image`}
                      className="bg-background text-muted-foreground absolute right-0.5 top-0.5 rounded-full border p-0.5 opacity-0 shadow-sm transition-opacity group-hover:opacity-100"
                    >
                      <XIcon className="size-3" />
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          )
        : single && (
            <div className="size-24 overflow-hidden rounded-md border">
              <AssetImage src={single} alt="" className="size-full object-cover" fallback={<ImageThumbFallback />} lightbox />
            </div>
          )}

      {multiple ? (
        <MultiValueInput
          values={values}
          options={[]}
          placeholder={t`Add path or URL`}
          ariaLabel={field.label}
          wikilinks={false}
          disabled={disabled}
          onChange={onChange}
        />
      ) : (
        <Input
          value={single}
          onChange={(event) => onChange(event.target.value || null)}
          placeholder={t`Vault path or URL`}
          aria-label={field.label}
          disabled={disabled}
        />
      )}

      {canUpload ? (
        <div
          onDragOver={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
          className={cn(
            "text-muted-foreground flex items-center justify-between gap-2 rounded-md border border-dashed px-3 py-2 text-xs transition-colors",
            dragging && "border-primary bg-primary/5",
          )}
        >
          <span className="truncate">
            {uploading ? "Uploading…" : "Drop an image or upload from your device"}
          </span>
          <input
            ref={inputRef}
            type="file"
            accept={IMAGE_ACCEPT}
            multiple={multiple}
            className="hidden"
            onChange={(event) => {
              void uploadFiles(Array.from(event.target.files ?? []));
              event.target.value = "";
            }}
          />
          <Button type="button" variant="outline" size="sm" disabled={uploading} onClick={() => inputRef.current?.click()}>
            {uploading ? (
              <Loader2Icon data-icon="inline-start" className="animate-spin" />
            ) : (
              <UploadIcon data-icon="inline-start" />
            )}
            Upload
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function ImageThumbFallback() {
  return (
    <div className="bg-muted text-muted-foreground flex size-full items-center justify-center">
      <ImageIcon className="size-5" />
    </div>
  );
}
