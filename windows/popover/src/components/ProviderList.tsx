import { useEffect, type ReactNode } from "react";
import MdiChevronRight from "~icons/mdi/chevron-right";
import MdiPlus from "~icons/mdi/plus";
import { DragHandle } from "@/components/DragHandle";
import { ProviderIcon } from "@/components/ProviderIcon";
import { TruncatedText } from "@/components/TruncatedText";
import { SortableItem, VerticalDnd } from "@/components/dnd";
import { Switch } from "@/components/ui/switch";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Card, Layout, ProviderInfo, ProviderListRow } from "@/lib/types";
import { m } from "@/paraglide/messages.js";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { addAccountDecision, metricsCountText, multiAccountSubtitle, providerListRows, reorderProviderRows, sendCommand } from "../model.js";

interface ProviderListProps {
  cards: Card[];
  /** Why the last enable/disable failed, or "". */
  error: string;
  layout: Layout;
  onOpen: (id: string) => void;
  onReorder: (ids: string[]) => void;
  onToggle: (id: string, on: boolean) => void;
  providers: ProviderInfo[];
  /** The header's right side next to Detect (Settings' reset button). */
  headerExtra?: ReactNode;
}

const ADD_ACCOUNT_LABELS: Record<string, () => string> = { code: m.add_claude_code_account, desktop: m.add_claude_desktop_account, codex: m.add_codex_account };

/**
 * The one provider list (Customize and Settings → Providers share it): enabled
 * providers come first, in the dashboard's card order — dragging a row moves
 * the provider's whole cards — and the rest keep the catalog's order below.
 * Every enabled row is uniform: grip, mark, name, subtitle, a discreet "+"
 * that adds an account, the switch that writes the provider's `enabled` to
 * config.toml, and a chevron to the provider's detail. A provider's accounts
 * live in its subtitle; the accounts themselves are chosen there, in the
 * detail, not in this list.
 */
export function ProviderList({ cards, error, headerExtra, layout, onOpen, onReorder, onToggle, providers }: ProviderListProps) {
  // The host re-reads credentials only when asked, so ask whenever the list opens.
  useEffect(() => {
    sendCommand("providers");
  }, []);
  const { enabled, disabled } = providerListRows(providers, cards, layout);
  return (
    <div className="flex flex-col gap-[var(--header-card-gap)]">
      <div className="flex items-center justify-between gap-2">
        <div className="section-title">{m.providers()}</div>
        <div className="flex items-center gap-2">
          {headerExtra}
          <button type="button" className="text-[length:var(--sz-badge)] text-[var(--accent)]" onClick={() => sendCommand("detect")}>
            {m.detect_providers()}
          </button>
        </div>
      </div>
      {enabled.length + disabled.length > 0 ? (
        <div className="card-surface">
          <VerticalDnd
            items={enabled.map((row) => row.id)}
            onReorder={(ids) => onReorder(reorderProviderRows(layout.cardOrder, enabled, ids))}
            overlay={(id) => {
              const row = enabled.find((item) => item.id === id);
              if (!row) return null;
              return (
                <div className="lifted-surface">
                  <ProviderRow onOpen={onOpen} onToggle={onToggle} row={row} />
                </div>
              );
            }}
          >
            {enabled.map((row) => (
              <SortableItem key={row.id} id={row.id}>
                {({ attributes, listeners }) => (
                  <ProviderRow handle={{ attributes, listeners }} onOpen={onOpen} onToggle={onToggle} row={row} />
                )}
              </SortableItem>
            ))}
            {disabled.map((row) => (
              <ProviderRow key={row.id} off onOpen={onOpen} onToggle={onToggle} row={row} />
            ))}
          </VerticalDnd>
        </div>
      ) : null}
      <div className={cn("px-[var(--card-pad)] text-[length:var(--sz-badge)]", error ? "text-meter-red" : "text-label-2")}>
        {error || m.all_providers_hint()}
      </div>
    </div>
  );
}

interface ProviderRowProps {
  handle?: Parameters<typeof DragHandle>[0];
  /** Drawn in the disabled block: faded, no drag handle, an inert chevron. */
  off?: boolean;
  onOpen: (id: string) => void;
  onToggle: (id: string, on: boolean) => void;
  row: ProviderListRow;
}

/**
 * One provider row, uniform across the enabled block: grip, mark, name +
 * subtitle, the discreet "+" (a direct action when the catalog offers one
 * account kind, a menu when it offers several, absent otherwise — its space
 * is kept so the columns line up), the switch, the chevron. The subtitle
 * reads "N metrics" for one card and "Default, work · 6 metrics" for
 * several; the catalog's status stands in while the provider has no card.
 */
function ProviderRow({ row, handle, off = false, onOpen, onToggle }: ProviderRowProps) {
  const { language } = useI18n();
  const first = !off && row.cards.length > 0 ? row.cards[0] : null;
  const statusText = row.state === "off" ? m.provider_off()
    : row.state === "off-ready" ? m.provider_off_ready()
      : row.state === "ready" ? m.provider_ready()
        : row.state === "needs" ? row.signIn
          : "";
  // A provider that is on but still lacks its credential keeps the hint (and
  // its action) even once the report brings its error card: "0 metrics" in
  // its place told nothing and made the hint flash and vanish.
  const metrics = !off && row.cards.length >= 1 && row.state !== "needs";
  const subtitle = metrics && row.cards.length === 1
    ? metricsCountText(row.cards[0].metrics, language)
    : metrics
      ? multiAccountSubtitle(row.cards, row.metrics, row.name, language)
      : statusText;
  const subtitleTone = metrics || row.state !== "needs" ? "text-label-2" : "text-meter-yellow";
  const action = row.state === "needs" && row.action
    ? row.action === "terminal" ? m.provider_sign_in()
      : row.action === "app" ? m.provider_open_app()
        : m.provider_add_key()
    : "";
  const add = addAccountDecision(row);
  const identity = (
    <>
      <ProviderIcon className="text-label-2" size="var(--row-icon-box)" slug={row.icon} title={row.name} />
      <span className="flex min-w-0 flex-col">
        <TruncatedText className="text-[length:var(--sz-header)] font-semibold">{row.name}</TruncatedText>
        {subtitle ? (
          <span className={cn("text-[length:var(--sz-badge)] break-words", subtitleTone)}>{subtitle}</span>
        ) : null}
      </span>
    </>
  );
  return (
    <div data-provider-id={row.id} className={cn("flex flex-col", off && "opacity-55")}>
      <div className="flex items-center gap-[var(--row-gap)] px-[var(--card-pad)] py-[var(--pad-control)]">
        {handle ? (
          <DragHandle attributes={handle?.attributes} listeners={handle?.listeners} />
        ) : (
          // Rows without a drag target (off block, overlay) get the same 16×22
          // footprint as the grip, so the name line stays aligned — no handle.
          <span className="grip" aria-hidden />
        )}
        {first ? (
          <button type="button" className="plain-btn hover-fill flex min-w-0 flex-1 items-center gap-[var(--row-gap)]" onClick={() => onOpen(first.id)}>
            {identity}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-[var(--row-gap)]">{identity}</div>
        )}
        {action ? (
          <button type="button" className="action-btn" onClick={() => sendCommand("sign-in-provider", { id: row.id })}>
            {action}
          </button>
        ) : null}
        {add ? (
          add.kind === "direct" ? (
            <button type="button" aria-label={m.add_account()} className="plain-btn hover-icon grid size-[var(--row-icon-box)] place-items-center" onClick={() => sendCommand("add-account", { kind: add.value })}>
              <MdiPlus className="size-[var(--icon-row)] text-label-2" />
            </button>
          ) : (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button type="button" aria-label={m.add_account()} className="plain-btn hover-icon grid size-[var(--row-icon-box)] place-items-center">
                  <MdiPlus className="size-[var(--icon-row)] text-label-2" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" sideOffset={2}>
                {add.value.map((kind) => (
                  <DropdownMenuItem key={kind} onSelect={() => sendCommand("add-account", { kind })}>
                    <MdiPlus />
                    <span className="flex-1">{(ADD_ACCOUNT_LABELS[kind] || m.add_account)()}</span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          )
        ) : (
          // No account kinds (or the provider is off): keep the column so the
          // switch and chevron stay aligned across rows.
          <span aria-hidden className="size-[var(--row-icon-box)]" />
        )}
        {row.switchable ? (
          <Switch
            checked={row.enabled}
            aria-label={`${m.provider_enable()} ${row.name}`}
            onCheckedChange={(on) => onToggle(row.id, on === true)}
          />
        ) : (
          // A custom provider toggles in config.toml by hand: keep the slot so
          // the chevron stays in its column.
          <span aria-hidden className="w-[var(--switch-w)]" />
        )}
        {first ? (
          <button type="button" aria-label={`${m.open()} ${row.cards.length > 1 ? row.name : first.title}`} className="plain-btn hover-icon grid size-[var(--row-icon-box)] place-items-center" onClick={() => onOpen(first.id)}>
            <MdiChevronRight className="size-[var(--icon-row)] text-label-3" />
          </button>
        ) : (
          // Nothing to open (the provider is off, or has no card yet): the
          // chevron stays, inert and dimmed, so every switch keeps its column.
          <span aria-hidden className="grid size-[var(--row-icon-box)] place-items-center">
            <MdiChevronRight className="size-[var(--icon-row)] text-label-3 opacity-40" />
          </span>
        )}
      </div>
    </div>
  );
}
