#!/bin/sh
set -e
mkdir -p vendor
cat > vendor/LICENSE <<'EOF'
Parcelo SDK licence. Customers of Parcelo may use, copy and study this SDK to integrate their own systems with the
Parcelo service. Redistribution outside the customer's organisation is not permitted.
EOF
cat > vendor/parcelo-sdk.min.js <<'EOF'
!function(e){var t="https://api.parcelo.example",n="2024-06-01";function r(r,o,a,i){var s={"Authorization":"Bearer "+r,"Parcelo-Version":n,"Content-Type":"application/json"};i&&(s["Idempotency-Key"]=i);return fetch(t+o,{method:a.m||"GET",headers:s,body:a.b?JSON.stringify(a.b):void 0}).then(function(e){if(429===e.status)throw new Error("rate");if(!e.ok)return e.json().then(function(t){throw t.error});return e.json()})}e.Parcelo=function(e){this.k=e},e.Parcelo.prototype.shipments={},e.Parcelo.prototype.list=function(e){var t=new URLSearchParams;e&&e.cursor&&t.set("starting_after",e.cursor),t.set("limit",String(e&&e.limit||20));return r(this.k,"/v1/shipments?"+t,{})},e.Parcelo.prototype.create=function(e,t){return r(this.k,"/v1/shipments",{m:"POST",b:{to:e.to,parcel:{weight_g:e.weightGrams},service:e.service||"standard"}},t)}}(window);
EOF
