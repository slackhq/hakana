$lilstring = "";

$n = new SimpleXMLElement($lilstring);
$n = $n->children();

if (!$n) {
    echo "false";
}