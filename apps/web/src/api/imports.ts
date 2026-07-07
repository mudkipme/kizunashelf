import {
  cancelImportJob,
  commitImportJob,
  createImportJob,
  getImportJob,
  listImportJobs,
  listImportSources,
  type CommitImportJobRequest,
  type CreateImportJobRequest,
} from "@kizunashelf/api-contract";
import { queryOptions } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";

export function fetchImportSources(init?: RequestInit) {
  return listImportSources(init, apiFetch);
}

export function fetchImportJob(id: string, init?: RequestInit) {
  return getImportJob(id, init, apiFetch);
}

// The (in-memory, server-side) job registry. Used to recover an in-progress job
// after a page reload, since the wizard otherwise holds the active job id only
// in local state.
export function fetchImportJobs(init?: RequestInit) {
  return listImportJobs(init, apiFetch);
}

export function startImportJob(request: CreateImportJobRequest) {
  return createImportJob(request, undefined, apiFetch);
}

export function commitImport(id: string, request: CommitImportJobRequest) {
  return commitImportJob(id, request, undefined, apiFetch);
}

export function stopImportJob(id: string) {
  return cancelImportJob(id, undefined, apiFetch);
}

// The import-source catalog (input kind, credentials, availability). Rarely
// changes within a session, so a long staleTime avoids refetching on remount.
export function importSourcesQuery() {
  return queryOptions({
    queryKey: ["importSources"],
    queryFn: ({ signal }) => fetchImportSources({ signal }),
    staleTime: 60_000,
  });
}
