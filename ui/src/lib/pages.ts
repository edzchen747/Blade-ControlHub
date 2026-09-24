export type PageId = "dashboard" | "lighting" | "keys" | "system" | "command-lab";

export const PAGES: Array<{ id: PageId; label: string; experimental?: boolean }> = [
  { id: "dashboard", label: "Dashboard" },
  { id: "lighting", label: "Lighting" },
  { id: "keys", label: "Keys" },
  { id: "system", label: "System" },
  { id: "command-lab", label: "Command Lab", experimental: true },
];
