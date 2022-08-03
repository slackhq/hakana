$a = rand(0, 1) ? "a" : "b";
$b = rand(0, 1) ? "a" : "b";

$s = rand(0, 1) ? $a : $b;
if (rand(0, 1)) $s = "c";

if ($s === $a) {
} else if ($s === $b) {}