/* SPDX-License-Identifier: GPL-2.0-or-later
 * Part of ftar. Links against OpenFTA (C) 2005 FSC Limited, GPL-2.0-or-later. */
/* Headless driver for the OpenFTA C core: load a tree, run one analysis,
 * print the report file the GUI would have shown. Replaces the JNI layer. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "TopLevelData.h"
#include "treeP.h"
#include "tree_pic.h"
#include "tree_pic_forest.h"
#include "symbol.h"
#include "symbol_defs.h"
#include "Canvas.h"
#include "CanvasP.h"
#include "file_pd.h"
#include "analysis_pd.h"
#include "CutSetsCommon.h"
#include "event_list.h"
#include "treeUtil.h"
#include "util.h"
#include "StringUtilities.h"
#include <time.h>

TREE tree;
TREE_PIC *tree_pict;
TREE_PIC_FOREST tree_pic_forest;
PRIMARY_EVENTS_DATABASE *current_database;
PRIMARY_EVENT *selected_event;

/* The probability analysis reads the cut sets of the tree (`tree->mcs_expr`).
 * The GUI gets them by running the cut sets analysis, which saves them to
 * TREE.mcs, and its probability dialog then reads that file back
 * (`nativeGetMCSFile`, NativeNumericalProbabilityDialog.c). The same here;
 * the cut sets report of that first run is not printed. */
static int quiet = 0;

static int load_mcs(void) {
    char *tree_filename = SU_Join(tree.path, tree.name);
    char *mcs_file = NULL;
    int num_mcs = 0, order = 0;
    time_t date;
    Expr e = NULL;
    file_get_mcs(tree_filename, &mcs_file, &num_mcs, &order, &date, &e);
    strfree(tree_filename);
    if (!mcs_file) { fprintf(stderr, "no cut sets file\n"); return 0; }
    expand_tree(&tree);
    if (tree.mcs_expr) ExprDestroy(tree.mcs_expr);
    tree.mcs_expr = e;
    tree.max_order = order;
    tree.num_mcs = num_mcs;
    tree.num_lambda_events = 0;
    count_lambda_events(&tree, tree.top_item);
    return 1;
}

static void show(const char *title, const char *file) {
    FILE *f;
    int c;
    if (quiet) return;
    f = fopen(file, "rb");
    fprintf(stderr, "report: %s (%s)\n", title, file);
    if (!f) { fprintf(stderr, "cannot open %s\n", file); return; }
    while ((c = fgetc(f)) != EOF) putchar(c);
    fclose(f);
}

int main(int argc, char **argv) {
    int s;
    if (argc < 3) {
        fprintf(stderr, "usage: openfta-run TREE.fta cutsets ORDER | prob ORDER UNIT TERMS | mc N UNIT\n");
        return 2;
    }
    tree_new(&tree);
    tree.database = NULL;
    replant_tree(&tree);
    tree_pict = (TREE_PIC *)malloc(sizeof(TREE_PIC));
    tree_pict->tree = &tree;
    tree_pict->selected_item = NULL;
    tree_pict->scale = 100;
    tree_pict->canvas = CanvasCreate();
    tree_pict->canvas->type = JAVA_CANVAS;
    tree_pict->set_no = 0;
    update_tree(tree_pict);
    tree_pic_forest.tree_pic = tree_pict;
    tree_pic_forest.global_tree_pic = NULL;
    for (s = SYMB_BASIC; s < SYMB_INVALID; s++)
        tree_pict->scaled_symbols[s] = scale_symbol(symbol_defs[s], (float)tree_pict->scale);

    file_load(&tree_pic_forest, argv[1]);

    if (!strcmp(argv[2], "cutsets") && argc >= 4) {
        CutSetWidAlgebraicOkCb(tree_pict, atoi(argv[3]));
    } else if (!strcmp(argv[2], "prob") && argc >= 6) {
        quiet = 1;
        CutSetWidAlgebraicOkCb(tree_pict, atoi(argv[3]));
        quiet = 0;
        if (!load_mcs()) return 1;
        ProbsWidOkCb(tree_pict, atoi(argv[3]), (float)atof(argv[4]), atoi(argv[5]));
    } else if (!strcmp(argv[2], "mc") && argc >= 5) {
        expand_tree(&tree);   /* MonteCarloDialog.nativeGetTreeInfo */
        MonteWidOkCb(tree_pict, atoi(argv[3]), (float)atof(argv[4]));
    } else {
        fprintf(stderr, "bad arguments\n");
        return 2;
    }
    return 0;
}

/* --- what the Java GUI provided ------------------------------------- */
void GenerateCutSetsPostScrolledText(const char *t, const char *f) { show(t, f); }
void GenerateNumericalProbabilityPostScrolledText(const char *t, const char *f) { show(t, f); }
void GenerateMonteCarloPostScrolledText(const char *t, const char *f) { show(t, f); }

/* What PEDFrame.java does: FTAFrame.loadPEDFile -> PEDFrame.loadPEDFile ->
 * nativeLoadPEDFile -> primary_events_database_open_cb; newPEDFile ->
 * nativeNewPEDFile(false) -> PEDFrameNewPEDFile. */
#include "primary_events_database.h"
#include "PED_shell.h"
void FTAFrameLoadPEDFile(const char *file) {
    if (!primary_events_database_open_cb((char *)file))
        fprintf(stderr, "could not open database %s\n", file);
}
void FTAFrameNewPEDFile(void) {
    if (primary_events_database_new_cb(current_database, FALSE))
        current_database = ped_shell_create();
}
