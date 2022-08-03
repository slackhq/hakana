final class MyException extends \Exception {
    public static function hello(): MyException
    {
        return new MyException();
    }
}

function sumExpectedToNotBlowPowerFuse(int $first, int $second)[]: int {
    $sum = $first + $second;
    if ($sum > 9000) {
        throw MyException::hello();
    }
    if ($sum > 900) {
        throw new MyException();
    }
    return $sum;
}