/// The minimum a step needs to be named in the header.
export type LabeledStep = {
  label: string;
  live?: boolean;
};

/// What the header names while work is in flight.
///
/// The last step still marked live is the one the turn is waiting on. Falls
/// back to the newest step, because one tool can finish before the next is
/// recorded and a stale name beats an empty header.
export function currentStepLabel(steps: readonly LabeledStep[]): string {
  for (let i = steps.length - 1; i >= 0; i -= 1) {
    if (steps[i].live) {
      return steps[i].label;
    }
  }

  return steps[steps.length - 1]?.label ?? "";
}
