-- Due to a bug introduced in `df8e4e6c88a8bfb6c1ec6b07dcda64093b477cbe`, IPv6
-- addresses in the configuration file might have ended up in the database
-- tagged with `type = "dns"`, which is incorrect. The bug was fixed in
-- `a2e72b48e79d090d33f6c13c485239947de0522e`.
-- However, Radicle 1.7.0 was released in between those two versions, so a
-- this migration to repair the entries is added.
-- To repair, set the type of DNS addresses that look like IPv6 addresses in
-- square brackets back to IPv6.
-- Even though DNS record names generally may contain '[' and ']:`, hostnames
-- for dialing should not. Even if, this case is probably extremely rare.
update or ignore addresses set type = 'ipv6' where type = 'dns' and instr(value, '[') = 1 and instr(value, ']:') > 1;
delete from addresses where type = 'dns' and instr(value, '[') = 1 and instr(value, ']:') > 1;