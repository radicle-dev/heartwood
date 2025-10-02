-- every valid IPv6 textual representation (including `::`) contains
-- at least one `:`. So if the bracket-stripped inner part has no `:`, the value
-- can't be a real IPv6 address.
delete from addresses
where type = 'ipv6'
  and instr(value, '[') = 1
  and instr(value, ']:') > 1
  and instr(substr(value, 2, instr(value, ']:') - 2), ':') = 0;