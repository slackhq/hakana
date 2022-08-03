$concat = "";
foreach (vec["x", "y"] as $v) {
    if ($concat != "") {
        $concat .= ", ";
    }
    $concat .= "($v)";
}