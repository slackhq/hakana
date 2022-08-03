$a = dict[];

foreach (vec["one", "two", "three"] as $key) {
    $a[$key] += rand(0, 10);
}

$a["four"] = true;

if ($a["one"]) {}