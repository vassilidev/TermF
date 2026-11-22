---
type: regex
pattern: 'IDENTIFIED WITH mysql_native_password BY|mysql_native_password\s*=\s*ON|(^|\n)\s*default_authentication_plugin\s*=\s*mysql_native_password'
flags: i
match: not_contains
target: last_message
---
