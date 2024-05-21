final class A {
    public function getUserId(AsyncMysqlConnection $conn) : void {
        $this->deleteUser(
            $conn,
            self::doFoo(),
            $this->getAppendedUserId((string) $_GET["user_id"])
        );
    }

    public function getAppendedUserId(string $user_id) : string {
        return "aaa" . $user_id;
    }

    public static function doFoo() : string {
        return "hello";
    }

    public function deleteUser(AsyncMysqlConnection $conn, string $userId, string $userId2) : void {
        $conn->query("delete from users where user_id = " . $userId);

        if (rand(0, 1)) {
            $conn->query("delete from users where user_id = " . $userId2);
        }
    }
}