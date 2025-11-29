type MyInt = int;
newtype UserId = int;

final class Container {
    public MyInt $value;
    public UserId $userId;
    const MyInt DEFAULT = 42;

    public function getValue(MyInt $param): MyInt {
        return $param;
    }

    public function getUserId(UserId $id): UserId {
        return $id;
    }
}
