import { uploadAsset } from "@/api/entities";

/** Chunk the byte conversion so large device images cannot overflow the stack. */
export async function uploadImageFile(entityId: string, field: string, file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  const result = await uploadAsset(entityId, {
    field,
    dataBase64: btoa(binary),
    contentType: file.type || undefined,
    filename: file.name || undefined,
  });
  return result.path;
}
