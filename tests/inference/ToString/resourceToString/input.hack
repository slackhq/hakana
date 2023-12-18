$a = fopen("php://memory", "r");
if ($a === false) exit();
$b = (string) $a;