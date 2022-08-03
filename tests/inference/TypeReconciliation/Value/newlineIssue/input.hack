$a = "foo";
$b = "


";

$c = $a;
if (rand(0, 1)) {
    $c = $b;
}

if ($c === $b) {}