# Official legal sources: endpoints, commands, terms

Read before calling an API or choosing an MCP server. Checked on the official pages on 2026-11-22; the Cellar
commands were run that day. The PISTE calls could not be run without an account: their URLs, bodies and headers
come from the DILA's examples and FAQ, the Cour de cassation's README and CGU, and the token endpoints answered
`invalid_client` to an empty request.

Contents: PISTE · Légifrance API · JudiLibre · Cellar · EUR-Lex web service · Without credentials · MCP servers

## PISTE (the State's API portal, run by the AIFE)

- Account: `https://piste.gouv.fr/registration`. A sandbox application is created with the account; a
  production application is created by hand (Applications > create an application), then the API is ticked in
  each. Accept the API's terms in each environment (API > Consentement CGU API), or every call answers 403.
- OAuth 2.0 client credentials. Token: `POST https://oauth.piste.gouv.fr/api/oauth/token` (sandbox:
  `https://sandbox-oauth.piste.gouv.fr/api/oauth/token`), form body `grant_type=client_credentials`,
  `client_id`, `client_secret`, `scope=openid`. The answer holds `access_token` and `expires_in` (3600 s in the
  DILA's example). Each call sends `Authorization: Bearer <token>`.
- Sandbox and production have their own client id and secret; a sandbox token is refused in production.
- Variable names TermF proposes: `PISTE_CLIENT_ID`, `PISTE_CLIENT_SECRET` (production credentials).
- Quotas: per second, minute or day, per application or per method, not published; on PISTE, open the
  application, then Actions > Consulter les quotas. Production quotas are higher than the sandbox's.
- Terms: PISTE CGU `https://piste.gouv.fr/images/com_apiportal/CGU/cgu_portal_FR.pdf`, plus each API's own.

Token and call in one shell call (the token never printed; nothing runs when the token is missing):

```sh
tok=$(curl -sS --proto '=https' https://oauth.piste.gouv.fr/api/oauth/token \
  --data-urlencode grant_type=client_credentials --data-urlencode scope=openid \
  --data-urlencode "client_id=$PISTE_CLIENT_ID" --data-urlencode "client_secret=$PISTE_CLIENT_SECRET" \
  | sed -nE 's/.*"access_token" *: *"([^"]+)".*/\1/p')
if [ -n "$tok" ]; then
  curl -sS --proto '=https' -X POST \
    https://api.piste.gouv.fr/dila/legifrance/lf-engine-app/consult/getArticle \
    -H "Authorization: Bearer $tok" -H 'Accept: application/json' -H 'Content-Type: application/json' \
    -d '{"id":"LEGIARTI000049568614"}'
else
  echo "no PISTE token: check the credentials, the environment and the API terms on PISTE"
fi
```

## Légifrance API (DILA): codes, laws, decrees, Journal officiel, collective agreements

- Base: `https://api.piste.gouv.fr/dila/legifrance/lf-engine-app` (sandbox:
  `https://sandbox-api.piste.gouv.fr/dila/legifrance/lf-engine-app`). Stable version since 2023-04-04.
- `GET /list/ping`: checks the token and the route.
- `POST /search`: body `{"fond": …, "recherche": {…}}`. Funds used in the DILA's examples: `CODE_ETAT` (codes
  by legal state), `CODE_DATE` (codes at a date), `LODA_ETAT` and `LODA_DATE` (laws, ordinances, decrees),
  `KALI` (collective agreements; field `IDCC` or `TITLE`), `JURI` (judicial decisions), `CETAT` (Conseil
  d'État). An article of a code at a date:

  ```json
  {"fond": "CODE_DATE",
   "recherche": {
     "champs": [{"typeChamp": "NUM_ARTICLE", "operateur": "ET",
                 "criteres": [{"typeRecherche": "EXACTE", "valeur": "L36-11", "operateur": "ET"}]}],
     "filtres": [{"facette": "NOM_CODE", "valeurs": ["Code des postes et des communications électroniques"]},
                 {"facette": "DATE_VERSION", "singleDate": "2026-11-22"}],
     "pageNumber": 1, "pageSize": 10, "operateur": "ET", "sort": "PERTINENCE", "typePagination": "ARTICLE"}}
  ```

  In `CODE_ETAT` the code's facet is `TEXT_NOM_CODE` and there is no date filter: read each result's state. A
  law by its number: `typeChamp` `NUM`, value `2019-290`, fund `LODA_ETAT`, filter `{"facette": "TEXT_LEGAL_STATUS",
  "valeur": "VIGUEUR"}`. `typeRecherche` values: `UN_DES_MOTS`, `EXACTE`, `TOUS_LES_MOTS_DANS_UN_CHAMP`,
  `AUCUN_DES_MOTS`, `AUCUNE_CORRESPONDANCE_A_CETTE_EXPRESSION`. Dates: `"YYYY-MM-DD"` or milliseconds since
  1970 (both appear in the DILA's examples).
- `POST /consult/getArticle` `{"id": "LEGIARTI…"}`: one version of an article, with its state and dates.
- `POST /consult/legiPart` `{"textId": "LEGITEXT…", "date": <ms>}`: a whole text at a date.
- Journal officiel: `POST /consult/lastNJo` `{"nbElement": 5}` (under 2,500) gives JORFCONT ids;
  `POST /consult/jorfCont` `{"id": "JORFCONT…", "pageNumber": 1, "pageSize": 10}` lists its JORFTEXT ids;
  `POST /consult/jorf` reads a text. JOs before June 2004 have no HTML text in the API.
- Every method, field and response is in the Swagger on PISTE (API catalogue, Légifrance); the sort and filter
  table and the examples are linked from `https://www.legifrance.gouv.fr/contenu/pied-de-page/open-data-et-api`.
- Identifiers (DILA glossary): `LEGIARTI` one version of an article; `LEGITEXT` a consolidated text; `LEGISCTA`
  a section; `CID` the identifier common to all versions; `JORFTEXT` a text as published in the JO.
- States: `V` in force; `VT` in force with a known end date; `VD` in force later; `Ab` repealed; `M` modified
  (a newer version exists); `MMN` modified or repealed before coming into force, so never in force.
- Pages to cite: `https://www.legifrance.gouv.fr/codes/article_lc/<LEGIARTI>`,
  `https://www.legifrance.gouv.fr/loda/article_lc/<LEGIARTI>`, `https://www.legifrance.gouv.fr/loda/id/<JORFTEXT>`.
- Terms: Licence Ouverte 2.0 (commercial reuse allowed), the API's CGU of 2022-12-15
  (`https://www.legifrance.gouv.fr/contenu/Media/files/pied-de-page/cgu-legifrance-api-vf-15-12-2022_0.pdf`).
  The data are not opposable: only the signed PDF of the Journal officiel is. Production aims at 95 %
  availability a day; the sandbox is used for the DILA's own tests and can be down without notice.
- Bulk XML (LEGI, JORF, KALI, CNIL…): `https://echanges.dila.gouv.fr/OPENDATA/`.

## JudiLibre (Cour de cassation): French case law

- Base: `https://api.piste.gouv.fr/cassation/judilibre/v1.0` (sandbox:
  `https://sandbox-api.piste.gouv.fr/cassation/judilibre/v1.0`). OpenAPI 1.2.5:
  `https://github.com/Cour-de-cassation/judilibre-search/blob/master/public/JUDILIBRE-public.json`.
- Access: tick JudiLibre in the same PISTE application and accept its terms; the CGU say each request is
  authenticated by OAuth 2.0, so the Bearer token above works for it. The README also shows a `KeyId` header
  (the application's API key) as an alternative.
- `GET /search?query=<words>`: filters `jurisdiction` (empty = Cour de cassation `cc`; values from
  `GET /taxonomy?id=jurisdiction`), `chamber`, `type`, `publication`, `solution`, `date_start` and `date_end`
  (ISO-8601), `sort`, `order`, `page_size` (50 at most, 10 by default), `page` (from 0).
- `GET /decision?id=<id>&resolve_references=true`: the full pseudonymised text with `jurisdiction`,
  `chamber`, `formation`, `decision_date`, `number`, `ecli`, `publication`, `solution`, `visa` (the texts
  applied), `update_date`, and `zones` (introduction, exposé, moyens, motivations, dispositif).
- `GET /taxonomy?id=<list>` lists the codes used by the filters; `GET /healthcheck`. `/export` and `/scan`
  are bulk exports: not for answering a question.
- Terms (Cour de cassation CGU, version of 2026-02-09): Licence Ouverte 2.0; for each decision reused, give the
  court, its formation, its seat and its date, the source and the date of the data's last update; never change
  the pseudonymisation; no reuse of judges' or clerks' identities to evaluate, compare or predict their
  practice (code de l'organisation judiciaire, art. L. 111-13). The API is labelled beta, with no availability
  guarantee; quotas on PISTE under the application's selected APIs > Actions > Consulter les quotas.

## Cellar (Publications Office of the EU): EU law, no key

- SPARQL: `GET https://publications.europa.eu/webapi/rdf/sparql?query=<urlencoded>` with
  `Accept: application/sparql-results+json`. The CELEX literal needs its type. Tested:

  ```sh
  q='PREFIX cdm: <http://publications.europa.eu/ontology/cdm#>
  PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
  SELECT ?inforce ?eli ?cons ?date WHERE {
    ?act cdm:resource_legal_id_celex "32016R0679"^^xsd:string .
    OPTIONAL { ?act cdm:resource_legal_in-force ?inforce }
    OPTIONAL { ?act cdm:resource_legal_eli ?eli }
    OPTIONAL { ?c cdm:act_consolidated_consolidates_resource_legal ?act ;
                  cdm:resource_legal_id_celex ?cons ; cdm:act_consolidated_date ?date .
               FILTER(STRSTARTS(STR(?cons), "02016R0679-")) }
  } ORDER BY DESC(?date) LIMIT 1'
  curl -sS --proto '=https' -G https://publications.europa.eu/webapi/rdf/sparql \
    --data-urlencode "query=$q" -H 'Accept: application/sparql-results+json'
  ```

  Result on 2026-11-22: in force `1`, ELI `http://data.europa.eu/eli/reg/2016/679/oj`, latest consolidated
  version `02016R0679-20160504`. Other useful properties of an act: `resource_legal_date_entry-into-force`,
  `resource_legal_date_end-of-validity`, `resource_legal_repeals_resource_legal`.
- From an ELI to the CELEX: `?work owl:sameAs <http://publications.europa.eu/resource/eli/reg/2016/679/oj> ;
  cdm:resource_legal_id_celex ?celex` (prefix `owl: <http://www.w3.org/2002/07/owl#>`).
- The text, by content negotiation on `https://publications.europa.eu/resource/celex/<CELEX>` with
  `Accept-Language: fra` (three-letter code) and one of: `Accept: application/xhtml+xml` (XHTML, also for
  consolidated versions), `Accept: application/zip;mtype=fmx4` (Formex XML in a zip),
  `Accept: application/xml;notice=object` (the metadata notice). The answer is a 303 to an `http://` URL:

  ```sh
  loc=$(curl -sS --proto '=https' -o /dev/null -w '%{redirect_url}' \
    -H 'Accept: application/xhtml+xml' -H 'Accept-Language: fra' \
    https://publications.europa.eu/resource/celex/02016R0679-20160504)
  curl -sS --proto '=https' -o regulation-2016-679-fr.xhtml "${loc/#http:/https:}"
  ```

- CELEX: sector `3` for legal acts (`32016R0679`: R regulation, L directive, D decision), `0` plus `-YYYYMMDD`
  for a consolidated version, `6` for EU case law.
- Links to cite: `https://eur-lex.europa.eu/legal-content/FR/TXT/?uri=CELEX:<CELEX>`; the ELI
  `http://data.europa.eu/eli/<type>/<year>/<number>/oj` is the act as published, and without `/oj` it leads to
  the latest consolidated version when there is one.
- Terms: reuse under Commission Decision 2011/833/EU, commercial or not; consolidated texts and summaries under
  CC BY 4.0 (name the source, say what changed); metadata CC0. Only the Official Journal is authentic.
- Bulk: a data dump of the acts in force per language (EU Login account), `https://datadump.publications.europa.eu`.

## EUR-Lex web service

SOAP, free after registration: an EU Login account, then the form at
`https://eur-lex.europa.eu/protected/web-service-registration.html`. Same query options as EUR-Lex's expert
search, full text included, answers in XML; it does not download documents (use Cellar). An account unused for
six months is deleted 30 days after a warning. Use it only for a full-text search Cellar's metadata cannot do;
its manual (version 2.01) could not be read on 2026-11-22: check on
`https://eur-lex.europa.eu/content/help/data-reuse/webservice.html`.

## Without credentials (France)

WebFetch on `https://www.legifrance.gouv.fr/...` pages: an article page shows the code or text, the article
number and "En vigueur depuis le …"; cite the page URL with that date. Same for `www.cnil.fr`,
`acpr.banque-france.fr`, `www.amf-france.org`, `www.arcom.fr`. Say in the report that the API was not used.

## MCP servers the user may set up

Checked on their repositories on 2026-11-22. A server's answer leads to a text; read and cite that text at the
official source.

- data.gouv.fr (official, MIT, `https://github.com/datagouv/datagouv-mcp`, last commit 2026-11-19): hosted at
  `https://mcp.data.gouv.fr/mcp`, no authentication (`claude mcp add --transport http datagouv
  https://mcp.data.gouv.fr/mcp`). Searches the open-data catalogue (datasets and APIs), not the texts of law.
- DroitFrancaisMCP (community, one maintainer, MIT, `https://github.com/jmtanguy/DroitFrancaisMCP`, last commit
  2025-11-21): Python; calls only the PISTE hosts above, with `PISTE_CLIENT_ID` and `PISTE_CLIENT_SECRET`
  (`PISTE_SANDBOX_…` for the sandbox), for Légifrance and JudiLibre.
- Légifrance has no official MCP server yet; the DILA announced on 2026-10-20 that it is working on one.
