class A {
    public function getUserId(AsyncMysqlConnection $conn) : void {
        $this->deleteUser(
            $conn,
            $this->getAppendedUserId((string) $_GET["user_id"])
        );
    }

    public function getAppendedUserId(string $user_id) : string {
        return "aaa" . $user_id;
    }

    public function deleteUser(AsyncMysqlConnection $conn, string $userId) : void {
        $userId2 = $userId;
        $conn->query("delete from users where user_id = " . $userId2);
    }
}