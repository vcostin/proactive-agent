# Platform readiness derivation interface

Type: grilling
Status: resolved
Blocked by: 02

## Question

Given the research on today’s duplication: what is the deepened readiness **interface** — how does `SetupStatus` (or its successor) derive from Platform `verify_catalog` + `required_for_*`, and what does the Setup Wizard keep as a presentation adapter vs drop?

Stay inside this map’s Platform section: readiness/gates only — not CatalogFetcher / download orchestration.

## Answer

Locked in [spec.md](../spec.md) Implementation Decisions → Platform readiness: derive SetupStatus from verify_catalog + required_for_*; Core ready still includes chat model; wizard is presentation adapter; fetch orchestration stays out.
