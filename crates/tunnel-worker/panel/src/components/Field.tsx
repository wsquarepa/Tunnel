import type { ComponentChildren } from "preact";

interface FieldProps {
  label: string;
  hint: string;
  children: ComponentChildren;
}

export function Field({ label, hint, children }: FieldProps) {
  return (
    <label class="field">
      <span class="field-label">{label}</span>
      {children}
      <span class="field-hint">{hint}</span>
    </label>
  );
}
