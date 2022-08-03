function takesArray(vec<string> $arr) : void {}

$a = vec[];
$a[] = 1;
$a[] = 2;

takesArray($a);