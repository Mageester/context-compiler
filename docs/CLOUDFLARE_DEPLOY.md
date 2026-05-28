# Cloudflare Pages — Custom Domain Setup

This document describes how to connect `ctx-compiler.getaxiom.ca` as a custom
domain for the **context-compiler** Cloudflare Pages project.

> **Status:** The CNAME record `ctx-compiler.getaxiom.ca → context-compiler.pages.dev`
> already exists in Cloudflare DNS. Only the Pages project dashboard registration
> step (step 4 below) is still needed.

---

## Prerequisites

- Cloudflare account with access to the **getaxiom.ca** zone.
- The **context-compiler** Pages project already deployed (Git-connected to
  `Mageester/context-compiler`, branch `main`, build output: `site/`).
- DNS CNAME record pre-configured:
  ```
  ctx-compiler.getaxiom.ca  CNAME  context-compiler.pages.dev
  ```

---

## Step-by-step

### 1. Log into the Cloudflare Dashboard

Go to [dash.cloudflare.com](https://dash.cloudflare.com) and sign in.

### 2. Navigate to Workers & Pages

In the left sidebar, click **Workers & Pages** (or use the account picker at
the top, then select **Workers & Pages** from the nav).

### 3. Select the context-compiler Pages project

Find **context-compiler** in the Pages list and click it to open the project
overview page.

### 4. Add the custom domain

In the left sidebar of the project overview, click **Custom domains**.

Click the **+ Add custom domain** button.

In the dialog that appears:

| Field                | Value                                    |
|----------------------|------------------------------------------|
| **Domain**           | `ctx-compiler.getaxiom.ca`               |
| **Zone**             | `getaxiom.ca` (auto-selected)            |

Click **Continue**.

Cloudflare will show a validation page. Because the CNAME record already
exists in DNS, verification will pass immediately.

Click **Activate domain**.

### 5. Confirm activation

After activation, the **context-compiler** project overview will list
`ctx-compiler.getaxiom.ca` under the **Custom domains** section with a status
of **Active**.

The site is now live at:

- **Custom:** https://ctx-compiler.getaxiom.ca
- **Auto-assigned:** https://context-compiler.pages.dev (also works)

### 6. Verify the site

```bash
curl -sI https://ctx-compiler.getaxiom.ca | head -5
```

Expected: HTTP/2 `200` (or `301` — the site redirects `/home` to `/`).

---

## Verifying DNS propagation

```bash
dig +short ctx-compiler.getaxiom.ca CNAME
```

Should return:

```
context-compiler.pages.dev.
```

---

## Troubleshooting

### "Domain already active on Pages"

This means the domain is active on a *different* Cloudflare Pages project.
Remove it from the old project first, or contact account admin.

### SSL certificate pending

Cloudflare provisions an SSL certificate after domain activation. It usually
completes within minutes. The site will still serve over HTTPS using a
Cloudflare edge certificate during provisioning.

### "Validation failed" / CNAME not found

Check that the CNAME record exists exactly as above:

```
ctx-compiler.getaxiom.ca  CNAME  context-compiler.pages.dev
```

The trailing dot (`.`) in DNS may be required: `context-compiler.pages.dev.`

Try removing and re-adding the CNAME record from the Cloudflare DNS tab
(**DNS → Records**).

---

## Summary of DNS records

| Type  | Name                        | Target                       | Proxy status |
|-------|-----------------------------|------------------------------|--------------|
| CNAME | `ctx-compiler.getaxiom.ca`  | `context-compiler.pages.dev` | DNS only     |

> **Note:** For Pages custom domains, proxying (orange cloud) is not required
> — Pages handles its own TLS termination. DNS-only (grey cloud) is correct.
