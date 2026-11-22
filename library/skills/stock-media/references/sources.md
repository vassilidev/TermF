# Sources: APIs, licences, terms

Read when you search, download or credit a file. Checked on the platforms' own pages on 2026-11-22; none has an official MCP server (community ones exist, small and unaudited: prefer the APIs).

## Openverse — photos, audio, no key

- Search: `GET https://api.openverse.org/v1/images/?q=<words>&license_type=commercial,modification` and `/v1/audio/`. Filters: `license` (`cc0,pdm,by,by-sa`…), `source`, `mature`.
- Anonymous: 20 requests a minute, 200 a day, `page_size` at most 20. A registered client (`POST /v1/auth_tokens/register/`, email verified, then `POST /v1/auth_tokens/token/` → `Authorization: Bearer <token>`) gets more.
- Per file: `license`, `license_version`, `license_url`, `creator`, `creator_url`, `attribution` (a ready credit line), `foreign_landing_url` (the source page: check the licence there), `url`, `provider`.
- Terms: attribution required; Openverse does not verify licences.

## Wikimedia Commons — photos, video, audio, no key

- Search with file info: `https://commons.wikimedia.org/w/api.php?action=query&format=json&generator=search&gsrnamespace=6&gsrsearch=<words>&prop=imageinfo&iiprop=url|mime|size|extmetadata&iiextmetadatafilter=LicenseShortName|LicenseUrl|Artist|AttributionRequired|UsageTerms`.
- `extmetadata`: `LicenseShortName` ("CC BY-SA 4.0"), `LicenseUrl` (absent for public domain), `Artist` (HTML: strip the tags), `AttributionRequired` ("true"/"false").
- A descriptive `User-Agent` with a contact is required (`<project>-video/1.0 (<project URL>)`); one request at a time, under five a second.
- Commons accepts no NC or ND file; trademarks and personality rights still apply.

## Pexels — photos, videos

- `GET https://api.pexels.com/v1/search?query=<words>&orientation=landscape` and `GET https://api.pexels.com/v1/videos/search?query=<words>`; header `Authorization: $PEXELS_API_KEY` (no "Bearer").
- 200 requests an hour, 20,000 a month.
- Credit fields: photos `photographer`, `photographer_url`; videos `user.name`, `user.url`.
- Licence: free, commercial use and changes allowed, attribution not required; not as part of a trademark, logo or business name, not resold unaltered, no implied endorsement by people or brands shown. API terms: a visible link to Pexels and credit to the photographer when possible ("Photo by X on Pexels").

## Pixabay — photos, videos

- `GET https://pixabay.com/api/?key=$PIXABAY_API_KEY&q=<words>&image_type=photo` and `https://pixabay.com/api/videos/?key=…`; music and sound effects are not in the API.
- 100 requests a minute; cache results for 24 hours; download the files, no permanent hotlinking; show where the media come from.
- Credit fields: `user`, `user_id` (`https://pixabay.com/users/<user>-<id>/`).
- Licence: free, no attribution required; not standalone resale, not in a trademark or logo, not commercial use of recognisable brands or trademarks shown.

## Unsplash — photos

- `GET https://api.unsplash.com/search/photos?query=<words>`; header `Authorization: Client-ID $UNSPLASH_ACCESS_KEY`.
- 50 requests an hour in demo mode, 1,000 once approved for production.
- On each download: `GET <photo.links.download_location>` with the key. Credit: `user.name` with a link to `user.links.html?utm_source=<app>&utm_medium=referral`, and Unsplash.
- Licence: commercial use allowed, attribution appreciated; not sold without significant change, not to build a competing service; no rights to trademarks or people shown; no use in machine-learning datasets.

## Freesound — sound effects (non-commercial API)

- `GET https://freesound.org/apiv2/search/?query=<words>&fields=id,name,username,license,previews,url`; `Authorization: Token $FREESOUND_API_KEY`.
- A token gives previews only; the original file needs OAuth2 (a user login). 60 requests a minute, 2,000 a day.
- Per-sound `license`: CC0, Attribution, Attribution NonCommercial (excluded). API terms: free for non-commercial purposes only; commercial use is agreed with Freesound.

## Jamendo — music (non-commercial API)

- `GET https://api.jamendo.com/v3.0/tracks/?client_id=$JAMENDO_CLIENT_ID&search=<words>&include=licenses&audiodownload_allowed=true`.
- Fields: `license_ccurl`, `artist_name`, `shareurl`, `audiodownload`.
- API terms: free for non-commercial use; any commercial use, a product video included, goes through Jamendo Licensing (a paid licence per project).

## CREDITS.md

```markdown
# Credits

| File | Title | Author | Source | Licence | Attribution | Changes | Date |
|---|---|---|---|---|---|---|---|
| media/pexels-1234567-office.jpg | Team at a desk | [Jane Doe](https://www.pexels.com/@jane) | [Pexels](https://www.pexels.com/photo/1234567/) | [Pexels License](https://www.pexels.com/license/) | Not required; credited | Cropped to 16:9 | 2026-11-22 |
| media/openverse-rain.mp3 | Soft rain | [J. Smith](https://example.org/smith) | [Openverse → Wikimedia](https://commons.wikimedia.org/wiki/File:Rain.ogg) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) | Required: "Soft rain by J. Smith, CC BY 4.0" | Trimmed, faded | 2026-11-22 |
```

For CC BY files, the attribution names the title, the author, the source and the licence with its link, and says what was changed.
