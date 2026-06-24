$a = "a";
$b = "b";

if (rand(0, 1) !== 0) {
    list($a, $b) = explode(".", "c.d");
}

echo $a;
echo $b;
