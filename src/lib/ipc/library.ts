// The document library, the file picker, and the attachments a message
// carries.
import { invoke } from "@tauri-apps/api/core";

export type FileInfo = {
  path: string;
  name: string;
  kind: string;
  ext: string;
  sz: number;
};

export type LibItem = {
  id: string;
  name: string;
  kind: string;
  ext: string;
  path: string;
  session_id: string | null;
  sz: number;
  created_at: string;
};

export function libraryList(kind?: string): Promise<LibItem[]> {
  return invoke<LibItem[]>("library_list", { kind: kind ?? null });
}

// Described where it stands, not copied: a second copy is the library's whole
// reason for not existing.
export function fileStat(path: string): Promise<FileInfo> {
  return invoke<FileInfo>("file_stat", { path });
}

export function librarySearch(query: string): Promise<LibItem[]> {
  return invoke<LibItem[]>("library_search", { query });
}

export function libraryPath(id: string): Promise<string> {
  return invoke<string>("library_path", { id });
}

export type LibPreview = {
  id: string;
  name: string;
  kind: string;
  ext: string;
  sz: number;
  text: string | null;
  truncated: boolean;
};

export type LibDownload = {
  id: string;
  dest: string;
};

export function libraryPreview(
  id: string,
  maxChars?: number,
): Promise<LibPreview> {
  return invoke<LibPreview>("library_preview", { id, maxChars });
}

export function libraryDownload(id: string): Promise<LibDownload> {
  return invoke<LibDownload>("library_download", { id });
}

// A library id and enough to show a chip. No path, because a path can move.
export type Attachment = {
  path?: string;
  id?: string;
  name: string;
  kind: string;
  sz: number;
};
