import type { ComponentChildren } from "preact";

interface FieldProps {
  label: string;
  hint: string;
  children: ComponentChildren;
}

// A plain <div>, not a <label>: wrapping a <select> in a <label> makes the label
// re-dispatch the click to the control, opening then instantly closing the native
// dropdown. A div keeps the label/hint layout without that misbehavior.
export function Field({ label, hint, children }: FieldProps) {
  return (
    <div class="field">
      <span class="field-label">{label}</span>
      {children}
      <span class="field-hint">{hint}</span>
    </div>
  );
}
