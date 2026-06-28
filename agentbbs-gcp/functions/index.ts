/**
 * AgentBBS sysop-report Cloud Function (2nd gen, Pub/Sub triggered).
 *
 * Triggered by the `agentbbs-events` Pub/Sub topic. Each message carries a
 * base64-encoded JSON event (the same shape `agentbbs-core`'s `Event` and the
 * Rust `agentbbs-gcp` crate produce). This function decodes the event and folds
 * it into a single running `sysop_reports/latest` Firestore document.
 *
 * The aggregation MUST mirror the canonical Rust `ReportAggregator::aggregate`
 * in `agentbbs-gcp/src/aggregate.rs`: total, by_kind, warnings, criticals, and
 * a tail of recent {kind, subject} summaries.
 */

import { cloudEvent, CloudEvent } from "@google-cloud/functions-framework";
import { Firestore, FieldValue } from "@google-cloud/firestore";

/** Keep parity with `RECENT_LIMIT` in src/aggregate.rs. */
const RECENT_LIMIT = 20;

/** The single document this function maintains. */
const REPORT_COLLECTION = "sysop_reports";
const REPORT_DOC = "latest";

const firestore = new Firestore();

/** Event kinds whose severity is `warn` (mirrors EventKind::severity in core). */
const WARN_KINDS = new Set(["security", "moderation"]);

/** The decoded core event shape (snake_case, as core serializes it). */
interface AgentEvent {
  at: string;
  kind: string;
  agent: string | null;
  subject: string;
  detail: unknown;
}

interface EventSummary {
  kind: string;
  subject: string;
}

interface SysopReport {
  total: number;
  by_kind: Record<string, number>;
  warnings: number;
  criticals: number;
  recent: EventSummary[];
}

/** The Pub/Sub message envelope delivered to a CloudEvent function. */
interface PubSubMessage {
  message?: {
    data?: string;
  };
}

function emptyReport(): SysopReport {
  return { total: 0, by_kind: {}, warnings: 0, criticals: 0, recent: [] };
}

/**
 * Fold a single event into a report, returning the updated report. This is the
 * incremental mirror of the batch `aggregate()` in the Rust crate.
 */
function fold(report: SysopReport, event: AgentEvent): SysopReport {
  report.total += 1;
  report.by_kind[event.kind] = (report.by_kind[event.kind] ?? 0) + 1;

  // Severity classification mirrors EventKind::severity. There is currently no
  // kind that maps to `critical`; we keep the branch for forward-compat.
  if (WARN_KINDS.has(event.kind)) {
    report.warnings += 1;
  }

  report.recent.push({ kind: event.kind, subject: event.subject });
  if (report.recent.length > RECENT_LIMIT) {
    report.recent = report.recent.slice(report.recent.length - RECENT_LIMIT);
  }
  return report;
}

cloudEvent("aggregateSysopReport", async (event: CloudEvent<PubSubMessage>) => {
  const data = event.data?.message?.data;
  if (!data) {
    console.warn("pubsub message had no data; skipping");
    return;
  }

  let parsed: AgentEvent;
  try {
    const json = Buffer.from(data, "base64").toString("utf8");
    parsed = JSON.parse(json) as AgentEvent;
  } catch (err) {
    console.error("failed to decode/parse event", err);
    return;
  }

  const ref = firestore.collection(REPORT_COLLECTION).doc(REPORT_DOC);

  // Transactionally read-modify-write the running report so concurrent
  // deliveries do not clobber each other.
  await firestore.runTransaction(async (tx) => {
    const snap = await tx.get(ref);
    const current = (snap.exists ? (snap.data() as SysopReport) : emptyReport());
    // Defensive defaults in case the stored doc predates a field.
    current.total ??= 0;
    current.by_kind ??= {};
    current.warnings ??= 0;
    current.criticals ??= 0;
    current.recent ??= [];

    const updated = fold(current, parsed);
    tx.set(ref, {
      ...updated,
      updated_at: FieldValue.serverTimestamp(),
    });
  });

  console.log(`folded event kind=${parsed.kind} subject=${parsed.subject}`);
});
