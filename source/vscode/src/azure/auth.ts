// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

import { log } from "qsharp-lang";
import { EventType, sendTelemetryEvent, UserFlowStatus } from "../telemetry";
import { getRandomGuid } from "../utils";
import { azureRequest, AzureUris, ResponseTypes } from "./networkRequests";
import { EndEventProperties } from "./workspaceActions";

export const scopes = {
  armMgmt: "https://management.azure.com/user_impersonation",
  quantum: "https://quantum.microsoft.com/user_impersonation",
  chatApi: "https://api.quantum.microsoft.com/Chat.ReadWrite",
};

export async function getAuthSession(
  scopes: string[],
  associationId: string,
): Promise<vscode.AuthenticationSession> {
  const start = performance.now();
  sendTelemetryEvent(EventType.AuthSessionStart, { associationId }, {});
  log.debug("About to getSession for scopes", scopes.join(","));
  try {
    let session = await vscode.authentication.getSession("microsoft", scopes, {
      silent: true,
    });
    if (!session) {
      log.debug("No session with silent request. Trying with createIfNone");
      session = await vscode.authentication.getSession("microsoft", scopes, {
        createIfNone: true,
      });
    }
    log.debug("Got session: ", JSON.stringify(session, null, 2));
    sendTelemetryEvent(
      EventType.AuthSessionEnd,
      {
        associationId,
        flowStatus: UserFlowStatus.Succeeded,
      },
      { timeToCompleteMs: performance.now() - start },
    );
    return session;
  } catch (e) {
    sendTelemetryEvent(
      EventType.AuthSessionEnd,
      {
        associationId,
        reason: "exception in getAuthSession",
        flowStatus: UserFlowStatus.Failed,
      },
      { timeToCompleteMs: performance.now() - start },
    );
    log.error("Exception occurred in getAuthSession: ", e);
    throw e;
  }
}

export async function getTenantIdAndToken(
  endEventProperties: EndEventProperties,
) {
  const associationId = endEventProperties.associationId;

  // For the MSA case, you need to query the tenants first and get the underlying AzureAD
  // tenant for the 'guest' MSA. See https://stackoverflow.microsoft.com/a/76246/108570
  const firstAuth = await getAuthSession([scopes.armMgmt], associationId);

  if (!firstAuth) {
    log.error("No authentication session returned");
    endEventProperties.reason = "no auth session returned";
    endEventProperties.flowStatus = UserFlowStatus.Failed;
    vscode.window.showErrorMessage(
      "Authentication failed or permission was denied",
    );
    return;
  }

  const firstToken = firstAuth.accessToken;

  const tenants: ResponseTypes.TenantList = await azureRequest(
    AzureUris.tenants(),
    firstToken,
    associationId,
  );
  log.trace(`Got tenants: ${JSON.stringify(tenants, null, 2)}`);
  if (!tenants?.value?.length) {
    log.error("No tenants returned");
    endEventProperties.reason = "no tenants exist for account";
    endEventProperties.flowStatus = UserFlowStatus.Failed;
    vscode.window.showErrorMessage(
      "There a no tenants listed for the account. Ensure the account has an Azure subscription.",
    );
    return;
  }

  // Quick-pick if more than one
  let tenantId = tenants.value[0].tenantId;
  if (tenants.value.length > 1) {
    const pickItems = tenants.value.map((tenant) => ({
      label: tenant.displayName,
      detail: tenant.tenantId,
    }));
    const choice = await vscode.window.showQuickPick(pickItems, {
      title: "Select a tenant",
    });
    if (!choice) {
      endEventProperties.reason = "aborted tenant choice";
      endEventProperties.flowStatus = UserFlowStatus.Aborted;
      return;
    }
    tenantId = choice.detail;
  }

  // *** Sign-in to that tenant and query the subscriptions available for it ***

  // Skip if first token is already for the correct tenant and for AAD.
  let tenantAuth = firstAuth;
  const matchesTenant = tenantAuth.account.id.startsWith(tenantId);
  const accountType = (tenantAuth as any).account?.type || "";
  if (accountType !== "aad" || !matchesTenant) {
    tenantAuth = await getAuthSession(
      [scopes.armMgmt, `VSCODE_TENANT:${tenantId}`],
      associationId,
    );
    if (!tenantAuth) {
      endEventProperties.reason =
        "authentication session did not return a value";
      endEventProperties.flowStatus = UserFlowStatus.Aborted;
      // The user may have cancelled the login
      log.debug("No AAD authentication session returned during 2nd auth");
      vscode.window.showErrorMessage(
        "Unable to authenticate to the Azure subscription account",
      );
      return;
    }
  }
  const tenantToken = tenantAuth.accessToken;
  return { tenantId, tenantToken };
}

export async function getTokenForWorkspace(workspace: {
  apiKey?: string;
  tenantId: string;
}) {
  // If using an API key, just return that as the 'token'
  if (workspace.apiKey) return `apiKey=${workspace.apiKey}`;

  const associationId = getRandomGuid();

  const workspaceAuth = await getAuthSession(
    [scopes.quantum, `VSCODE_TENANT:${workspace.tenantId}`],
    associationId,
  );
  return workspaceAuth.accessToken;
}
