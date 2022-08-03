$ds = ldap_connect('example.com');
$dn = 'o=Psalm, c=US';
$filter = ldap_escape($_GET['filter']);
ldap_search($ds, $dn, $filter, []);