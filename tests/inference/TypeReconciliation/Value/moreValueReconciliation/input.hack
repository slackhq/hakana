$a = rand(0, 1) !== 0 ? "a" : "b";
$b = rand(0, 1) !== 0 ? "a" : "b";

$s = rand(0, 1) !== 0 ? $a : $b;
if (rand(0, 1) !== 0) $s = "c";

if ($s === $a) {
} else if ($s === $b) {}
