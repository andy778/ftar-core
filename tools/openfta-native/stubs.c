/* SPDX-License-Identifier: GPL-2.0-or-later
 * Part of ftar. Links against OpenFTA (C) 2005 FSC Limited, GPL-2.0-or-later. */
/* No-op stand-ins for the Java GUI callbacks. Messages go to stderr. */
#include <stdio.h>
#include <string.h>
typedef int BOOL;
#define V(name) void name() {}
V(FTACanvasClearArc) V(FTACanvasDrawArc) V(FTACanvasDrawLine) V(FTACanvasDrawRect)
V(FTACanvasEditGate) V(FTACanvasEditIntermediateEvent) V(FTACanvasEditTransferTree)
V(FTACanvasEditTreeText) V(FTACanvasFillArc) V(FTACanvasFillPolyline) V(FTACanvasFillRect)
V(FTACanvasSelectPrimaryEvent) V(FTACanvasSetPenColour) V(FTACanvasSetPenWidth)
V(FTACanvasWriteText) V(FTAFrameInvokeCutSetsDialog) V(FTAFrameInvokeMonteCarloDialog)
V(FTAFrameInvokeNumericalProbabilityDialog)
V(FTAFramePrintTextFile) V(FTAFrameSetSensitive) V(FTAFrameSetWindowTitle)
V(GenerateCutSetsProgressBarInc) V(GenerateCutSetsSetProgressBarMax)
V(GenerateMonteCarloProgressBarInc) V(GenerateMonteCarloSetProgressBarMax)
V(GenerateNumericalProbabilityCloseWorkingDialog) V(GenerateNumericalProbabilityProgressBarInc)
V(GenerateNumericalProbabilitySetProgressBarMax) V(PEDFrameSetWindowTitle)
int FTACanvasFontHeight() { return 12; }
int FTACanvasTextWidth(const char *t) { return 7 * (int)strlen(t); }
static void msg(const char *kind, const char *a, const char *b) { fprintf(stderr, "%s: %s | %s\n", kind, a ? a : "", b ? b : ""); }
void FTAFramePostError(const char *w, const char *t) { msg("error", t, w); }
void FTAFramePostWarning(const char *w, const char *t) { msg("warning", t, w); }
void FTAFramePostScrolledText(const char *t, const char *f) { msg("text", t, f); }
void GateDialogPostError(const char *w, const char *t) { msg("error", t, w); }
void PEDFramePostError(const char *w, const char *t) { msg("error", t, w); }
void GenerateNumericalProbabilityPostWarning(const char *t, const char *w) { msg("warning", t, w); }
BOOL FTAFrameAskQuestion(const char *q, const char *t) { msg("question (answered no)", t, q); return 0; }
BOOL PEDFrameAskQuestion(const char *q, const char *t) { msg("question (answered no)", t, q); return 0; }
BOOL GenerateCutSetsCheckForInterrupt() { return 0; }
BOOL GenerateMonteCarloCheckForInterrupt() { return 0; }
BOOL GenerateNumericalProbabilityCheckForInterrupt() { return 0; }
char *PEDFrameGetSelectedId() { return NULL; }
