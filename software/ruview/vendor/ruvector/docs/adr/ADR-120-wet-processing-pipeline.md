# ADR-120: WET Processing Pipeline for Medical + CS Corpus Import

**Status:** Phase 1 Deployed
**Date:** 2026-03-22
**Updated:** 2026-03-22
**Author:** ruvector team

## Context

The CDX HTML extractor is broken -- it returns empty titles from Wayback Machine content due to inconsistent HTML structure across archived pages. Fixing the extractor would require handling thousands of edge cases across decades of web standards.

Common Crawl provides WET (Web Extracted Text) files that contain pre-extracted plain text. These files bypass all HTML parsing entirely.

## Decision

Process Common Crawl WET files instead of fixing the CDX HTML extractor for the medical + CS corpus import pipeline.

## Architecture

```
Download WET segment (~150MB gz)
  -> gunzip (streaming)
  -> Filter by 30 medical + CS domains
  -> Chunk content (300-8000 chars)
  -> Tag by domain + content keywords
  -> Batch inject into pi.ruv.io brain (10 items/batch)
```

### Components

| Script | Purpose |
|--------|---------|
| `scripts/wet-processor.sh` | Downloads and processes a single WET segment |
| `scripts/wet-filter-inject.js` | Parses WARC WET format, filters by domain, injects to brain |
| `scripts/wet-orchestrate.sh` | Orchestrates multi-segment processing |
| `scripts/wet-job.yaml` | Cloud Run Job config for parallel processing |

### Target Domains (60+)

**Medical:** pubmed, ncbi, who.int, cancer.org, aad.org, skincancer.org, dermnetnz.org, melanoma.org, mayoclinic.org, clevelandclinic.org, medlineplus.gov, cdc.gov, nih.gov, nejm.org, thelancet.com, bmj.com, webmd.com, healthline.com, medscape.com, jamanetwork.com, cochrane.org, clinicaltrials.gov, fda.gov, mskcc.org, mdanderson.org, nccn.org, asco.org, esmo.org, dana-farber.org, cancer.net, uptodate.com

**CS/Research:** nature.com, sciencedirect.com, arxiv.org, acm.org, ieee.org, dl.acm.org, proceedings.neurips.cc, openreview.net, paperswithcode.com, huggingface.co, pytorch.org, tensorflow.org, cs.stanford.edu, deepmind.google, research.google, microsoft.com/research, frontiersin.org, plos.org, biomedcentral.com, cell.com, springer.com, wiley.com, elsevier.com, mdpi.com, aaai.org, usenix.org, jmlr.org, aclanthology.org

## Rationale

- WET files contain pre-extracted text -- no HTML parsing needed
- 100x faster than CDX+HTML extraction pipeline
- Same S3 cost model (public bucket, no auth)
- Each WET segment is ~150MB compressed, ~100K pages
- Streaming pipeline keeps memory usage under 1GB

## Cost Estimate

- ~90,000 WET segments across crawls 2020-2026
- Filter reduces to ~0.1% relevant pages (medical + CS domains)
- Estimated ~$200 total in compute (Cloud Run) for full corpus
- 6 weeks at 5 segments/day for complete import

## Consequences

- Bypasses HTML parsing entirely (positive)
- Text quality depends on Common Crawl's extraction (acceptable)
- No images or structured HTML elements (acceptable for text corpus)
- Requires streaming to handle 150MB+ files without memory issues (handled)

## Implementation Status (2026-03-22)

### Local Processing Results

| Segments | Records Scanned | Domain Matches | Injected | Errors |
|----------|----------------|----------------|----------|--------|
| 8 segments (CC-MAIN-2026-08) | ~170K pages | 109 | 109 | 1 |

Match rate: ~0.06% (109 medical/CS pages from 170K total pages).

### Cloud Run Job Deployment

| Component | Status |
|-----------|--------|
| `deploy-wet-job.sh` | Created — builds Docker image with baked-in paths + filter script |
| `wet-full-import.sh` | Created — orchestrates 14 quarterly crawls (2020-2026) |
| Domain list | Expanded to 60+ medical + CS domains |
| Cloud Run Job `wet-import-n202608` | Deployed, 50 segments, 10 parallel, executing |

### Issues Encountered and Fixed

1. **Paths file corruption**: Initial deployment baked XML error response into `paths.txt` due to GCS auth failure. Fixed by using `curl` to fetch paths directly from `data.commoncrawl.org`.
2. **Task index off-by-one**: `CLOUD_RUN_TASK_INDEX` is 0-based, `sed -n` is 1-based. Fixed with `$((TASK_IDX + 1))p`.
3. **Domain comma splitting**: `gcloud --set-env-vars` splits on commas. Fixed by using `--env-vars-file` (YAML format).
4. **gsutil unavailable**: `node:20-alpine` lacks gsutil. Fixed by baking all files into the Docker image at build time.

### Brain Growth

| Metric | Before WET | After WET (local) | Growth |
|--------|-----------|-------------------|--------|
| Memories | 1,659 | 1,768 | +109 |
| Graph edges | 444,663 | 565,357 | +121K |
| Sparsifier | 29.4x | 39.8x | +35% better |
| Contributors | 84 | 85 | +1 |
| Knowledge velocity | 0 | 188 | Active |
| Temporal deltas | 0 | 188 | Tracking |

### Projected Full Import

| Phase | Crawls | Segments | Est. Pages | Time | Cost |
|-------|--------|----------|-----------|------|------|
| Current | CC-MAIN-2026-08 | 50 | ~750 | Hours | ~$5 |
| Week 1 | + 2025, 2024, 2023 | 300 | ~4,500 | Days | ~$30 |
| Month 1-2 | + 2020-2022 | 600 | ~9,000 | Weeks | ~$60 |
| Full (100K segs) | All 14 crawls | 14,000 | ~200K+ | Months | ~$750 |
