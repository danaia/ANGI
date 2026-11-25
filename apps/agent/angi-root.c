/* Full administrator execution for ANGI's explicitly authorized admin group.
 * Keep this small executable on the system partition: NVMe is mounted nosuid. */
#define _GNU_SOURCE
#include <errno.h>
#include <grp.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <syslog.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    uid_t caller = getuid();
    struct group *admin = getgrnam("agx-admin");
    gid_t groups[256];
    int count = getgroups(256, groups);
    int authorized = caller == 0;
    if (admin && getgid() == admin->gr_gid) authorized = 1;
    for (int i = 0; admin && i < count; ++i)
        if (groups[i] == admin->gr_gid) authorized = 1;
    if (!authorized || geteuid() != 0) {
        fputs("angi-root requires the installed setuid helper and agx-admin membership.\n", stderr);
        return 1;
    }
    if (argc < 2) {
        fputs("Usage: angi-root COMMAND [ARGUMENT ...]\n", stderr);
        return 2;
    }
#ifdef ANGI_DATA_VOLUME_ONLY
    if (argc < 4 || strcmp(argv[1], "/usr/bin/python3") ||
        strcmp(argv[2], "/usr/libexec/agx-agent/data_volume.py") ||
        !((argc == 4 && (!strcmp(argv[3], "list") || !strcmp(argv[3], "list-picker"))) ||
          (argc == 5 && (!strcmp(argv[3], "mount") || !strcmp(argv[3], "format"))))) {
        fputs("This ANGI administrator helper only manages the data drive.\n", stderr);
        return 2;
    }
#endif
    /* No caller-controlled loader, interpreter, shell-startup or credential
     * environment crosses the privilege boundary. cwd and explicit argv do. */
    if (clearenv() || setenv("PATH", "/usr/sbin:/usr/bin:/sbin:/bin", 1) ||
        setenv("HOME", "/root", 1) || setenv("USER", "root", 1) ||
        setenv("LOGNAME", "root", 1) || setenv("LANG", "C.UTF-8", 1) ||
        setenv("TERM", "xterm-256color", 1) || setgroups(0, NULL) ||
        setgid(0) || setuid(0)) {
        perror("angi-root");
        return 1;
    }
    openlog("angi-root", LOG_PID, LOG_AUTH);
    syslog(LOG_NOTICE, "administrator uid=%lu command=%s", (unsigned long)caller, argv[1]);
    closelog();
#ifdef ANGI_DATA_VOLUME_ONLY
    char *const command[] = { argv[1], "-I", argv[2], argv[3], argv[4], NULL };
    execv(command[0], command);
#else
    execvp(argv[1], argv + 1);
#endif
    int error = errno;
    perror("angi-root exec");
    return error == ENOENT ? 127 : 126;
}
