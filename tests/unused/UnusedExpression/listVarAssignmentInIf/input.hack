$a = "a";
$b = "b";

if (rand(0, 1)) {
    list($a, $b) = explode(".", "c.d");
}

echo $a;
echo $b;