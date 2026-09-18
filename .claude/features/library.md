# Library — Document Generation

Code: `src-tauri/src/library/{mod.rs,store.rs,schema.rs,doc.rs}`, `src/lib/ipc.ts:410-454`, `src/components/{LibraryPage,LibraryCard,DocViewer}.tsx`, `src/stores/docViewer.ts`, `src/components/agent/DocumentCard.tsx`.

## Backend

- Commands: `library_add, library_list, library_get, library_delete, library_search, library_path, library_download, library_preview, library_create_doc`.
- `doc.rs: kind_of(): docx|pdf|pptx|xlsx|csv|md|txt` (images `None`), hand-rolled `ZipWriter + crc32 + esc_xml` for OOXML. `default_dir()` sync in `lib.rs:53-54`.
- Tool: `doc.create{name, kind, title, content}` in `tools/mod.rs`.

## Frontend

- `LibItem, LibPreview, LibDownload + libraryList/Search/Path/Preview/Download` in `ipc.ts`. `LibraryPage + LibraryCard + DocViewer` + `docViewer.ts` store + `DocumentCard` agent block.

## Rules for agents

- Keep OOXML writer dependency-free (no new crates for zip/xml without discussion).
- `kind_of()` closed set; images return `None` — do not silently mislabel.
- Tests: `src-tauri/tests/library_doc_test.rs`.
