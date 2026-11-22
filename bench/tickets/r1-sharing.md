# Share a list with other people, with roles

**Type:** feature, security sensitive (access rights)

Today a list belongs to the account that created it and nobody else ever sees it. People want to share a list with
their family or their team: some of them change the tasks, others only follow them. The owner invites them with a
link that expires, and decides who stays.

## Acceptance criteria

1. Besides its owner, a list has members, each with a role: `editor` or `viewer`. The owner keeps every right on the
   list.
2. The owner creates an invitation link: `POST /api/lists/:id/invitations` with
   `{"role": "viewer" | "editor", "expires_in_hours": <number, optional: 72 by default, at most 720>}` answers `201`
   with `{"id", "token", "url", "role", "expires_at"}`. `url` is the link to send: it opens the page, which accepts
   the invitation once the person is signed in (or has created an account).
3. `POST /api/invitations/:token/accept`, by a signed-in user, makes them a member of the list with the invitation's
   role and answers `200` with the list.
4. The owner sees the invitations still pending with `GET /api/lists/:id/invitations`, and revokes one with
   `DELETE /api/lists/:id/invitations/:invitationId` (`204`).
5. `GET /api/lists/:id/members` answers `200` with `[{"id": <user id>, "username", "role": "owner" | "editor" | "viewer"}]`,
   the owner included. The owner removes a member with `DELETE /api/lists/:id/members/:userId` (`204`).
6. `GET /api/lists` also answers the lists shared with the user. Every list it answers, and `GET /api/lists/:id`,
   carries the caller's `role`: `owner`, `editor` or `viewer`.
7. Editors read the list and create, change and delete its tasks, and comment on them. Viewers only read: the list,
   its tasks, their comments, the activity. Renaming or deleting the list, inviting and removing members stay with the
   owner.
8. A task can be assigned to any member of its list, the owner included.
9. The page shows who a list is shared with and their role. The owner creates an invitation link and copies it,
   revokes pending invitations and removes members. Opening an invitation link accepts it.
10. The existing lists keep working as before for their owners.
11. Tests cover sharing; the README documents it.
