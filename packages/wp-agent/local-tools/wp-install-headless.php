<?php
/**
 * Führt die WordPress-Installation headless aus.
 * Legt Administrator, Site-Titel und Permalinks an.
 */

// WordPress verlangt einige Server-Variablen für den Installer.
$_SERVER['HTTP_HOST'] = 'localhost:81';
$_SERVER['SERVER_NAME'] = 'localhost';
$_SERVER['SERVER_PORT'] = '81';
$_SERVER['REQUEST_URI'] = '/wordpress/';
$_SERVER['SCRIPT_NAME'] = '/wordpress/wp-admin/install.php';
$_SERVER['PHP_SELF'] = '/wordpress/wp-admin/install.php';
$_SERVER['HTTPS'] = '';
$_SERVER['REQUEST_METHOD'] = 'GET';

define('WP_INSTALLING', true);
define('WP_ADMIN', true);

require_once __DIR__ . '/wp-load.php';
require_once ABSPATH . 'wp-admin/includes/upgrade.php';

echo "WordPress-Version: " . get_bloginfo('version') . "\n";

if (is_blog_installed()) {
    echo "Bereits installiert – keine Änderung.\n";
    exit(0);
}

$result = wp_install(
    'Wartungscockpit Test',
    'wartung',
    'wartung@example.local',
    true,
    '',
    'WartungCockpit!2026',
    'de_DE'
);

if (is_wp_error($result)) {
    echo "FEHLER: " . $result->get_error_message() . "\n";
    exit(1);
}

echo "Installation abgeschlossen. Admin-ID: " . $result['user_id'] . "\n";

// Permalinks: hübsche URLs, damit die REST-API sauber antwortet.
global $wp_rewrite;
$wp_rewrite->set_permalink_structure('/%postname%/');
$wp_rewrite->flush_rules(true);

update_option('blogdescription', 'Testinstanz für den WordPress-Agent');
update_option('timezone_string', 'Europe/Berlin');
update_option('date_format', 'd.m.Y');
update_option('time_format', 'H:i');

echo "Permalinks: " . get_option('permalink_structure') . "\n";
echo "WP_HOME: " . get_option('home') . "\n";
echo "WP_SITEURL: " . get_option('siteurl') . "\n";
echo "Fertig.\n";
