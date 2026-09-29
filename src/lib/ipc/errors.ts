// The error types the app branches on. A gateway failure and a session
// failure look the same to a caller that only cares that the call failed, so
// the two are separate types rather than one message convention.

export class IpcError extends Error {}
export class SessIpcError extends IpcError {}
export class GwIpcError extends IpcError {}
