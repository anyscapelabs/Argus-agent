// Dictation: local models, local audio. Nothing here sends a recording
// anywhere -- the base64 clip only ever crosses into the Rust process that
// hands it to whisper on this machine.
import { Channel, invoke } from "@tauri-apps/api/core";

import {
  CMD_VOICE_CANCEL,
  CMD_VOICE_CFG,
  CMD_VOICE_DELETE,
  CMD_VOICE_DOWNLOAD,
  CMD_VOICE_MODELS,
  CMD_VOICE_SET_CFG,
  CMD_VOICE_STATUS,
  CMD_VOICE_TRANSCRIBE,
} from "./commands";

export type VoiceConfig = {
  sttModel: string;
  language: string;
};

export type ModelInfo = {
  key: string;
  file: string;
  label: string;
  mb: number;
  note: string;
};

export type ModelRow = ModelInfo & {
  installed: boolean;
  active: boolean;
};

export type VoiceStatus = {
  ready: boolean;
  model: string;
  installed: boolean;
  vadInstalled: boolean;
  engineLoaded: boolean;
};

export type DownloadEvent =
  | { type: "progress"; key: string; got: number; total: number }
  | { type: "done"; key: string }
  | { type: "failed"; key: string; msg: string };

export function voiceConfig(): Promise<VoiceConfig> {
  return invoke<VoiceConfig>(CMD_VOICE_CFG);
}

export function voiceSetConfig(cfg: VoiceConfig): Promise<void> {
  return invoke<void>(CMD_VOICE_SET_CFG, { cfg });
}

export function voiceModels(): Promise<ModelRow[]> {
  return invoke<ModelRow[]>(CMD_VOICE_MODELS);
}

export function voiceStatus(): Promise<VoiceStatus> {
  return invoke<VoiceStatus>(CMD_VOICE_STATUS);
}

export function voiceDownloadModel(
  key: string,
  onEvent: Channel<DownloadEvent>,
): Promise<void> {
  return invoke<void>(CMD_VOICE_DOWNLOAD, { key, onEvent });
}

export function voiceDeleteModel(key: string): Promise<void> {
  return invoke<void>(CMD_VOICE_DELETE, { key });
}

export function voiceTranscribe(wavB64: string): Promise<string> {
  return invoke<string>(CMD_VOICE_TRANSCRIBE, { wavB64 });
}

export function voiceCancel(): Promise<void> {
  return invoke<void>(CMD_VOICE_CANCEL);
}