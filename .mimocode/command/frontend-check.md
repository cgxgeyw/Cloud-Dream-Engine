---
description: Run TypeScript type check and Vite build for the frontend to catch compilation and build errors.
---

# Frontend Build Check

Run the standard two-step validation for the React/TypeScript frontend:

## Step 1: TypeScript type check

```bash
cd frontend && npx tsc --noEmit 2>&1
```

If this reports errors, fix them before proceeding. Common patterns in this project:
- Missing imports after file moves
- Type mismatches in `apiAdapter.ts` or `types.ts`
- JSX closing tag issues from encoding damage

## Step 2: Vite build

```bash
cd frontend && npm run build 2>&1
```

This catches CSS import issues, missing modules, and asset problems that `tsc` alone misses.

## Quick combined (optional)

If you just need a fast pass and node/npm are on PATH:

```powershell
cd frontend; npx tsc --noEmit 2>&1 | Select-Object -First 30; if ($?) { npm run build 2>&1 }
```

Report both outputs. If TypeScript passes but build fails, the issue is likely in CSS imports or asset references.
