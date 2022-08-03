$s = rand(0,1) ? "strlen" : vec[1];
if (!is_callable($s)) {
    array_pop($s);
}