# Opnsense Unbound External-DNS Webhook

Webhook allowing external-dns to drive opnsense's unbound service

  **This is very dodgy code and i have very little time to take care of it. use it at your own risk**

## example of values.yaml used with Bitnami's helm chart for external dns

```yaml
provider: webhook
registry: noop
extraArgs:
  webhook-provider-url: http://localhost:8800
managedRecordTypesFilters:
  - A
  - AAAA
sources:
  - pod
  - service
sidecars:
  - name: opnsense-unbound-external-dns-webhook
    image: ghcr.io/jobs62/opnsense_unbound_external-dns_webhook:v0.2.0-rc1
    ports:
      - containerPort: 8800
        name: http
    env:
      - name: OPNSENSE_BASE
        value: "https://10.62.62.1/"
      - name: OPNSENSE_ALLOW_INVALID_CERTS
        value: "true"
      - name: OPNSENSE_DOMAIN_FILTERS
        value: "[\".home\"]"
      - name: OPNSENSE_KEY
        valueFrom:
          secretKeyRef:
            name: opnsense
            key: key
      - name: OPNSENSE_SECRET
        valueFrom:
          secretKeyRef:
            name: opnsense
            key: secret
```

## Thanks

- Ajpantuso
