function foo() : int
{
    $bitmask = 1;
    $bytes = 2;
    $ret = $bytes | ~$bitmask;
    return $ret;
}