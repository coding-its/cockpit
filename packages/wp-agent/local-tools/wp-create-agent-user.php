<?php
/**
 * Legt einen dedizierten Agent-Benutzer an und erzeugt ein Application Password.
 * Gibt Benutzername und Passwort einmalig auf der Konsole aus.
 */

$_SERVER['HTTP_HOST'] = 'localhost:81';
$_SERVER['SERVER_NAME'] = 'localhost';
$_SERVER['SERVER_PORT'] = '81';
$_SERVER['REQUEST_URI'] = '/wordpress/';
$_SERVER['SCRIPT_NAME'] = '/wordpress/wp-admin/';
$_SERVER['PHP_SELF'] = '/wordpress/wp-admin/';
$_SERVER['HTTPS'] = '';
$_SERVER['REQUEST_METHOD'] = 'GET';

require_once __DIR__ . '/wp-load.php';
require_once ABSPATH . 'wp-admin/includes/user.php';
// WP_Application_Passwords wird von wp-load.php bereits geladen
// (ab WP 7.x unter wp-includes/class-wp-application-passwords.php).

$login = 'wartung-cockpit';
$email = 'cockpit@example.local';

$user = get_user_by('login', $login);

if (!$user) {
    $user_id = wp_create_user($login, wp_generate_password(32), $email);
    if (is_wp_error($user_id)) {
        echo "FEHLER beim Anlegen: " . $user_id->get_error_message() . "\n";
        exit(1);
    }
    $user = new WP_User($user_id);
    $user->set_role('administrator');
    echo "Benutzer angelegt: $login (ID {$user_id})\n";
} else {
    echo "Benutzer existiert bereits: $login (ID {$user->ID})\n";
}

// Vorhandene Application Passwords dieses Benutzers entfernen, damit es eindeutig bleibt.
$existing = WP_Application_Passwords::get_user_application_passwords($user->ID);
foreach ($existing as $item) {
    WP_Application_Passwords::delete_application_password($user->ID, $item['uuid']);
}
if ($existing) {
    echo count($existing) . " altes Application Password entfernt.\n";
}

$created = WP_Application_Passwords::create_new_application_password(
    $user->ID,
    array('name' => 'Wartungscockpit Agent')
);

if (is_wp_error($created)) {
    echo "FEHLER beim Application Password: " . $created->get_error_message() . "\n";
    exit(1);
}

list($password, $item) = $created;

echo "USER=$login\n";
echo "APP_PASSWORD=$password\n";
echo "UUID={$item['uuid']}\n";
