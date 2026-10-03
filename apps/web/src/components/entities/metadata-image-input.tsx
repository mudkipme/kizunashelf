//! The editor for `image` / `imageList` fields.
//!
//! Images can arrive three ways — a vault-relative path, a remote URL, or bytes
//! dropped/picked from the host — and all three end up as the same frontmatter
//! value. Existing entries upload assets immediately and save their paths with
//! the draft; creation keeps device images local until Create.

import { Trans, useLingui } from "@lingui/react/macro";
import { ImageIcon, Loader2Icon, UploadIcon, XIcon } from "lucide-react";
import type { DragEvent } from "react";
import { useRef, useState } from "react";
import { toast } from "sonner";

import { AssetImage } from "@/components/assets/asset-image";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { uploadImageFile } from "@/lib/image-upload";
import { cn } from "@/lib/utils";

import { FormDisclosure } from "./form-disclosure";
import { listDisplayValues, uniqueStrings, valueToText } from "./frontmatter-utils";
import { MultiValueInput } from "./metadata-list-inputs";
import type { EditableFieldSpec, FrontmatterValue, PickImage } from "./metadata-types";

const IMAGE_ACCEPT = "image/*";
const IMAGE_EXTENSION = /\.(png|jpe?g|gif|webp|avif|bmp|svg|tiff?)$/i;

function isImageFile(file: File): boolean {
  return file.type.startsWith("image/") || IMAGE_EXTENSION.test(file.name);
}

/**
 * Editor for `image` / `imageList` fields: shows thumbnails of the current
 * value(s), keeps the raw path/URL entry (so vault assets and remote URLs still
 * work). Existing entries upload files into the vault; creation supplies a
 * picker callback that keeps them local until the entry is created.
 */
export function ImageFieldInput({
  field,
  value,
  disabled,
  entityId,
  onPickImage,
  onChange,
}: {
  field: EditableFieldSpec;
  value: FrontmatterValue | undefined;
  disabled: boolean;
  entityId?: string;
  onPickImage?: PickImage;
  onChange: (value: FrontmatterValue) => void;
}) {
  const { t } = useLingui();
  const multiple = field.kind === "imageList";
  const values = listDisplayValues(value, false);
  const single = multiple ? "" : valueToText(value);
  const [uploading, setUploading] = useState(false);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const canUpload = Boolean(entityId || onPickImage) && !disabled;
  const busy = disabled || uploading;
  const isPreview = (value: string) => Boolean(onPickImage) && value.startsWith("blob:");

  async function uploadFiles(files: File[]) {
    if (!canUpload || uploading) return;
    const images = files.filter(isImageFile);
    if (images.length === 0) {
      if (files.length > 0) toast.error(t`Only image files can be uploaded.`);
      return;
    }
    setUploading(true);
    try {
      const added: string[] = [];
      for (const file of multiple ? images : images.slice(0, 1)) {
        const path = onPickImage
          ? await onPickImage(field.key, file)
          : await uploadImageFile(entityId!, field.key, file);
        added.push(path);
        // Keep each completed upload if a later file in the batch fails.
        onChange(multiple ? uniqueStrings([...values, ...added]) : path);
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t`Upload failed.`);
    } finally {
      setUploading(false);
    }
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    if (canUpload && !uploading) void uploadFiles(Array.from(event.dataTransfer.files));
  }

  return (
    <div className="flex flex-col gap-2">
      {multiple
        ? values.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {values.map((item, index) => (
                <div
                  key={`${item}-${index}`}
                  className="group relative size-16 overflow-hidden rounded-md border"
                >
                  <AssetImage
                    src={item}
                    alt=""
                    className="size-full object-cover"
                    fallback={<ImageThumbFallback />}
                    lightbox
                  />
                  {!busy ? (
                    <button
                      type="button"
                      onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
                      aria-label={t`Remove image`}
                      className="absolute top-0.5 right-0.5 rounded-full border bg-background p-0.5 text-muted-foreground shadow-sm transition-opacity focus-visible:opacity-100 sm:opacity-0 sm:group-hover:opacity-100 pointer-coarse:opacity-100"
                    >
                      <XIcon className="size-3" />
                    </button>
                  ) : null}
                </div>
              ))}
            </div>
          )
        : single && (
            <div className="flex items-start gap-2">
              <div className="size-24 overflow-hidden rounded-md border">
                <AssetImage
                  src={single}
                  alt=""
                  className="size-full object-cover"
                  fallback={<ImageThumbFallback />}
                  lightbox
                />
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove image`}
                disabled={busy}
                onClick={() => onChange(null)}
              >
                <XIcon />
              </Button>
            </div>
          )}

      <FormDisclosure
        title={t({
          message: "Use a path or URL",
          comment:
            "Disclosure button revealing manual image path/URL entry as an alternative to the device image picker",
        })}
      >
        {multiple ? (
          <MultiValueInput
            values={values.filter((item) => !isPreview(item))}
            options={[]}
            placeholder={t`Add path or URL`}
            ariaLabel={field.label}
            wikilinks={false}
            disabled={busy}
            onChange={(next) => onChange([...values.filter(isPreview), ...next])}
          />
        ) : (
          <Input
            value={isPreview(single) ? "" : single}
            onChange={(event) => onChange(event.target.value || null)}
            placeholder={t`Vault path or URL`}
            aria-label={field.label}
            disabled={busy}
          />
        )}
      </FormDisclosure>
      {canUpload ? (
        <div
          onDragOver={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
          className={cn(
            "flex items-center justify-between gap-2 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground transition-colors",
            dragging && "border-primary bg-primary/5",
          )}
        >
          <span className="truncate">
            {uploading ? (
              <Trans>Preparing image…</Trans>
            ) : (
              <Trans>Drop an image or choose from your device</Trans>
            )}
          </span>
          <input
            ref={inputRef}
            type="file"
            accept={IMAGE_ACCEPT}
            multiple={multiple}
            className="hidden"
            tabIndex={-1}
            aria-label={t`Upload ${field.label}`}
            disabled={busy}
            onChange={(event) => {
              void uploadFiles(Array.from(event.target.files ?? []));
              event.target.value = "";
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={uploading}
            onClick={() => inputRef.current?.click()}
          >
            {uploading ? (
              <Loader2Icon data-icon="inline-start" className="animate-spin" />
            ) : (
              <UploadIcon data-icon="inline-start" />
            )}
            <Trans comment="Button that opens the device image picker in an entity form">
              Upload
            </Trans>
          </Button>
        </div>
      ) : null}
      {onPickImage && values.some(isPreview) ? (
        <p className="text-xs text-muted-foreground">
          <Trans>Selected images will be saved when you create the entry.</Trans>
        </p>
      ) : null}
    </div>
  );
}

function ImageThumbFallback() {
  return (
    <div className="flex size-full items-center justify-center bg-muted text-muted-foreground">
      <ImageIcon className="size-5" />
    </div>
  );
}
