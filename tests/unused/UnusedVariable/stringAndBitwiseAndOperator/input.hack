function randomBits() : string
{
    $bitmask = \chr(0xFF >> 1);

    $randomBytes    = random_bytes(1);
    $randomBytes[0] = $randomBytes[0] & $bitmask;

    return $randomBytes;
}