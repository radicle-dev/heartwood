```
$ rad did help

Do not require activated DID:

   list  list known DIDs, with status (cached, at which version?, private key found?)
         if there is an activated DID, it is printed first.
         This is the default if there is no active DID.

   init      initialize/incept a new DID
      [--from=<public-key>]
      [--from="\$(rad self --ssh-key)"] to bootstrap
      [--to=<public-key>] for pre-rotation (breaks bridging from self)

   activate  activate ("login") a particular DID

   cache     clear/refresh the DID cache, this could scan all seeded repos for DIDs
   log       shows the key events of 

Require activated DID:

   rotate  to rotate to a new (or the pre-rotated key)
   revoke  
   edit
   sign    an arbitrary message using DID (we might use this for releases)
   show    This is the default if there is an active DID.
```