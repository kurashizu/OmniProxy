"use client";
import { useEffect, useState } from "react";
import { Card } from "@/components/common/Card";
import { useAppStore } from "@/store/appStore";
import { t } from "@/lib/i18n";
import { ipc } from "@/lib/ipc";
import type { GuiConfig, NodeConfig, ProxyState } from "@/lib/schema";

export function NodeFormCard({
  initial,
  state,
  onSaved,
}: {
  initial: GuiConfig | null;
  state: ProxyState;
  onSaved?: (cfg: GuiConfig) => void;
}) {
  const locale = useAppStore((s) => s.locale);
  const [cfg, setCfg] = useState<NodeConfig | null>(null);
  const [showYaml, setShowYaml] = useState(false);
  const [showToken, setShowToken] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedTick, setSavedTick] = useState(0);
  const [configPath, setConfigPath] = useState<string>("");

  useEffect(() => {
    if (initial && cfg == null) {
      const node = initial.nodes[initial.active_node] ?? initial.nodes[0];
      if (node) setCfg({ ...node });
    }
  }, [initial, cfg]);

  useEffect(() => {
    ipc
      .defaultConfigPath()
      .then(setConfigPath)
      .catch(() => setConfigPath(""));
  }, []);

  if (!cfg) return null;

  const update = (patch: Partial<NodeConfig>) =>
    setCfg((c) => (c ? { ...c, ...patch } : c));

  const onSave = async (andRestart: boolean) => {
    if (!initial) return;
    setSaving(true);
    setError(null);
    try {
      const next: GuiConfig = {
        ...initial,
        nodes: initial.nodes.map((n, i) =>
          i === initial.active_node ? cfg : n,
        ),
      };
      await ipc.saveGuiConfig(next);
      if (andRestart && state.state === "running") {
        await ipc.stopProxy();
        await new Promise((r) => setTimeout(r, 200));
        await ipc.startProxy();
      }
      setSavedTick((n) => n + 1);
      onSaved?.(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card
      title={t("settings.title", locale)}
      extra={
        <div className="flex items-center gap-2 text-xs">
          <button
            onClick={() => setShowYaml((v) => !v)}
            className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
          >
            {t("settings.yaml", locale)}
          </button>
          <button
            onClick={() => ipc.openConfigDir().catch(() => {})}
            className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
          >
            {t("settings.openDir", locale)}
          </button>
        </div>
      }
    >
      {showYaml ? (
        <pre className="overflow-auto rounded border border-[#252934] bg-[#0f1115] p-3 text-[11px] text-[#9ca3af] font-mono">
          {configPath}
          {"\n"}
          {JSON.stringify({ nodes: [cfg], active_node: 0 }, null, 2)}
        </pre>
      ) : (
        <div className="flex flex-col gap-4">
          <Row label={t("node.name", locale)}>
            <input
              className="flex-1"
              value={cfg.name}
              onChange={(e) => update({ name: e.target.value })}
            />
          </Row>
          <Row label={t("settings.enabled", locale)}>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={cfg.enabled}
                onChange={(e) => update({ enabled: e.target.checked })}
              />
              <span>{cfg.enabled ? "✓" : "—"}</span>
            </label>
          </Row>

          <SectionLabel text={t("settings.section.server", locale)} />
          <Row label={t("node.server", locale)}>
            <div className="flex-1 flex flex-col gap-1">
              <input
                className="w-full"
                value={cfg.server}
                onChange={(e) => update({ server: e.target.value })}
                placeholder="example.com:443"
              />
              <span className="text-[11px] text-[#6b7280]">
                {t("node.serverHint", locale)}
              </span>
            </div>
          </Row>
          <Row label={t("settings.token", locale)}>
            <div className="flex-1 flex gap-2">
              <input
                className="flex-1"
                type={showToken ? "text" : "password"}
                value={cfg.token}
                onChange={(e) => update({ token: e.target.value })}
              />
              <button
                onClick={() => setShowToken((v) => !v)}
                className="rounded border border-[#252934] bg-[#0f1115] px-2 py-0.5 text-[11px] text-[#9ca3af] hover:text-[#e5e7eb] hover:border-[#3b82f6]"
              >
                {showToken ? t("settings.hide", locale) : t("settings.show", locale)}
              </button>
            </div>
          </Row>

          <SectionLabel text={t("settings.section.local", locale)} />
          <Row label={t("settings.socksPort", locale)}>
            <input
              type="number"
              min={1}
              max={65535}
              className="w-32"
              value={cfg.socks_port}
              onChange={(e) => update({ socks_port: Number(e.target.value) })}
            />
          </Row>
          <Row label={t("settings.adminPort", locale)}>
            <input
              type="number"
              min={2}
              max={65535}
              className="w-32"
              value={cfg.admin_port}
              onChange={(e) => update({ admin_port: Number(e.target.value) })}
            />
          </Row>
          <Row label={t("settings.physIp", locale)}>
            <div className="flex-1 flex gap-2">
              <input
                className="flex-1"
                value={cfg.phys_ip ?? ""}
                placeholder={t("settings.physIp.auto", locale)}
                onChange={(e) =>
                  update({ phys_ip: e.target.value === "" ? null : e.target.value })
                }
              />
            </div>
          </Row>

          <SectionLabel text={t("settings.section.tun", locale)} />
          <Row label={t("settings.tunName", locale)}>
            <input
              className="w-32"
              value={cfg.tun_name}
              onChange={(e) => update({ tun_name: e.target.value })}
            />
          </Row>
          <Row label={t("settings.tunIp", locale)}>
            <input
              className="w-40"
              value={cfg.tun_ip}
              onChange={(e) => update({ tun_ip: e.target.value })}
            />
            <span className="text-[#9ca3af] text-xs">/</span>
            <input
              type="number"
              min={1}
              max={32}
              className="w-16"
              value={cfg.tun_prefix}
              onChange={(e) => update({ tun_prefix: Number(e.target.value) })}
            />
          </Row>
          <Row label={t("settings.tunGw", locale)}>
            <input
              className="w-40"
              value={cfg.tun_gw}
              onChange={(e) => update({ tun_gw: e.target.value })}
            />
          </Row>
          <Row label={t("settings.tunIp6", locale)}>
            <input
              className="w-40"
              value={cfg.tun_ip6}
              onChange={(e) => update({ tun_ip6: e.target.value })}
            />
            <span className="text-[#9ca3af] text-xs">/</span>
            <input
              type="number"
              min={1}
              max={128}
              className="w-16"
              value={cfg.tun_prefix6}
              onChange={(e) => update({ tun_prefix6: Number(e.target.value) })}
            />
          </Row>
          <Row label={t("settings.tunGw6", locale)}>
            <input
              className="w-40"
              value={cfg.tun_gw6}
              onChange={(e) => update({ tun_gw6: e.target.value })}
            />
          </Row>

          {error && (
            <div className="rounded border border-[#ef4444]/30 bg-[#ef4444]/10 px-3 py-2 text-xs text-[#ef4444]">
              {error}
            </div>
          )}
          {savedTick > 0 && !error && (
            <div className="rounded border border-[#10b981]/30 bg-[#10b981]/10 px-3 py-2 text-xs text-[#10b981]">
              {t("settings.saved", locale)}
            </div>
          )}

          <div className="flex items-center justify-end gap-2">
            <button
              onClick={() => onSave(false)}
              disabled={saving}
              className="rounded-md border border-[#252934] bg-[#0f1115] px-3 py-1.5 text-sm text-[#e5e7eb] hover:border-[#3b82f6] disabled:opacity-50"
            >
              {saving ? t("settings.saving", locale) : t("settings.save", locale)}
            </button>
            <button
              onClick={() => onSave(true)}
              disabled={saving}
              className="rounded-md border border-[#3b82f6] bg-[#3b82f6] px-3 py-1.5 text-sm text-white hover:bg-[#2563eb] disabled:opacity-50"
            >
              {t("settings.saveRestart", locale)}
            </button>
          </div>
        </div>
      )}
    </Card>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-32 shrink-0 text-[#9ca3af] text-sm">{label}</span>
      <div className="flex-1 flex items-center gap-2">{children}</div>
    </div>
  );
}

function SectionLabel({ text }: { text: string }) {
  return (
    <div className="mt-2 text-[11px] uppercase tracking-wider text-[#6b7280]">
      {text}
    </div>
  );
}
