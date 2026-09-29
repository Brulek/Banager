import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { quitAnyway, quitKeptWaiting, quitQuestionShown } from "../lib/api";
import { freshOperations, queryKeys, useOperations } from "../lib/queries";
import { isActive, runsToItsEnd, useOperationName } from "../lib/operations";
import { QUIT_NO_CANCEL_KEYS, quitBodyKey, quitStops, tellRustTwice, useQuitRequests } from "../lib/quit";
import type { OpSummary } from "../lib/types";
import { WarningIcon } from "./icons";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";

/**
 * The question a quit asks while an operation is under way
 * (src-tauri/src/quit.rs): Rust has called the quit off and brought the
 * window back, and this asks, in the app's sheet (`Dialog`):
 * 「还有 2 个操作没完成」, a line on what quitting now does -- it stops them,
 * and, while a command is under way, the tool it works on can be left half
 * done (`quitBodyKey`) -- a line for each one that has started and that
 * nothing can stop, such as rustup's self update, which quitting does not
 * stop, and two buttons: 「仍然退出」, which cancels what can be cancelled,
 * waits for it to stop and quits (`quitAnyway`; the buttons stay held
 * meanwhile), and 「继续等待」, which leaves Canager running, has the focus
 * as the sheet opens, and is what Escape does.
 *
 * It goes by the operations as the backend lists them when Rust asks
 * (`freshOperations`): with none left undone by then, Canager quits
 * without asking, as the user asked. Asked, it counts them as they go on,
 * and goes away by itself once every one has finished: nothing is left to
 * wait for, and Canager stays, with how they went on the operation bar.
 *
 * Once it is on screen, it tells Rust so, by the number of the newest
 * question it answers (`quitQuestionShown`): Rust waits 2 seconds for
 * that, then quits, since a page that never showed the question is not
 * there for the user to answer. When it goes without quitting --
 * 「继续等待」, Escape, or everything having finished -- it tells Rust that
 * too (`quitKeptWaiting`), which stops that wait from quitting should the
 * first word not have got through. Each word is sent once more should it
 * fail (`tellRustTwice`).
 *
 * Mounted once, by `App`. Draws nothing until Rust asks.
 */
export function QuitQuestion() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: operations } = useOperations();
  const nameOf = useOperationName(operations);
  const [asked, setAsked] = useState(false);
  // The newest question from Rust that the sheet answers, by its number.
  const [question, setQuestion] = useState<number | null>(null);
  const [quitting, setQuitting] = useState(false);
  const keepWaitingButton = useRef<HTMLButtonElement>(null);

  const quit = useCallback(() => {
    setQuitting(true);
    quitAnyway().then(
      () => {
        setQuitting(false);
        setAsked(false);
      },
      (e: unknown) => {
        // Canager stays, and the question with it, to be answered again.
        console.error("quit_anyway failed", e);
        setQuitting(false);
      },
    );
  }, []);

  useQuitRequests((asking) => {
    const answer = (listed: OpSummary[] | undefined) => {
      if ((listed ?? []).some(isActive)) {
        setAsked(true);
        setQuestion(asking);
      } else quit();
    };
    freshOperations(queryClient).then(answer, (e: unknown) => {
      // The list as the page last heard it, then.
      console.error("list_operations failed", e);
      answer(queryClient.getQueryData<OpSummary[]>(queryKeys.operations));
    });
  });

  const active = (operations ?? []).filter(isActive);
  const count = active.length;
  const body = quitBodyKey(active);

  // It goes, and Canager stays: Rust is told, so that its wait for word
  // from the page does not quit.
  const keepWaiting = useCallback(() => {
    setAsked(false);
    if (question !== null) void tellRustTwice(() => quitKeptWaiting(question), "quit_kept_waiting");
  }, [question]);

  // Everything finished while it asked: it goes, and Canager stays -- and
  // it does not come back by itself when something starts later. Not while
  // Canager quits: 「仍然退出」 cancels them, and they finish on the way.
  useEffect(() => {
    if (asked && count === 0 && !quitting) keepWaiting();
  }, [asked, count, quitting, keepWaiting]);

  return (
    <Dialog
      open={asked && count > 0}
      onOpenChange={(open) => {
        if (!open) keepWaiting();
      }}
      title={t("quit.title", { count })}
      initialFocus={keepWaitingButton}
      footer={
        <>
          <button type="button" disabled={quitting} onClick={quit} className={BUTTON.large.grey}>
            {t("quit.quitAnyway")}
          </button>
          <button
            ref={keepWaitingButton}
            type="button"
            disabled={quitting}
            onClick={keepWaiting}
            className={BUTTON.large.default}
          >
            {t("quit.keepWaiting")}
          </button>
        </>
      }
    >
      {question !== null && <OnScreen question={question} />}
      {body !== null && (
        <p className="break-words text-body text-foreground">{t(body, { count: quitStops(active).length })}</p>
      )}
      {active.filter(runsToItsEnd).map((op) => (
        <p key={op.id} className="mt-3 flex gap-2 text-body text-foreground">
          <WarningIcon size={16} className="mt-px shrink-0 text-warning" />
          <span className="min-w-0 break-words">{t(QUIT_NO_CANCEL_KEYS[op.kind], { name: nameOf(op) })}</span>
        </p>
      ))}
    </Dialog>
  );
}

/**
 * Tells Rust that question `question` is on screen (`quitQuestionShown`),
 * so that Canager waits for the user's answer rather than quitting: drawn
 * inside the sheet, so that it says so once the sheet is in the page, and
 * again for each question that comes while it is up. Sent once more should
 * it fail; should that fail too, Canager quits 2 seconds after asking, as
 * it does with nobody here to answer, unless the user has answered
 * 「继续等待」 by then (`quitKeptWaiting`).
 */
function OnScreen({ question }: { question: number }) {
  useEffect(() => {
    void tellRustTwice(() => quitQuestionShown(question), "quit_question_shown");
  }, [question]);
  return null;
}
