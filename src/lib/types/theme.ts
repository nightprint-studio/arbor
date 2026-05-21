export interface Theme {
  id: string;
  name: string;
  description?: string;
  built_in: boolean;
  vars: Record<string, string>;
}
