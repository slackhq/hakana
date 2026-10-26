$lilstring = "";

$n = new SimpleXMLElement($lilstring);
$n = $n->children();

if ($n is null) {
    echo "false";
}