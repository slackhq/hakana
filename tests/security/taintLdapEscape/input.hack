$ds = ldap_connect('example.com');
$dn = 'o=Psalm, c=US';
$filter = ldap_escape(HH\global_get('_GET')['filter']);
ldap_search($ds, $dn, $filter, dict[]);