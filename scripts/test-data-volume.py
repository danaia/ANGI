#!/usr/bin/env python3
"""Exercise mount and format preflight without accessing any real drive."""
from contextlib import contextmanager, nullcontext
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/agent"))
import data_volume


class DataVolumeTests(unittest.TestCase):
    def setUp(self):
        temp = self.enterContext(tempfile.TemporaryDirectory())
        self.root = Path(temp)
        self.disk = self.root / "nvme0n1"
        self.partition = self.disk / "nvme0n1p1"
        for node, minor in ((self.disk, 0), (self.partition, 1)):
            (node / "holders").mkdir(parents=True)
            (node / "ro").write_text("0\n")
            (node / "dev").write_text(f"259:{minor}\n")
        (self.disk / "diskseq").write_text("7\n")
        (self.partition / "partition").write_text("1\n")

    def test_mounted_read_only_or_claimed_partition_blocks_whole_drive(self):
        self.assertTrue(data_volume.disk_available(self.disk, set()))
        self.assertFalse(data_volume.disk_available(self.disk, {"259:1"}))
        (self.partition / "ro").write_text("1\n")
        self.assertFalse(data_volume.disk_available(self.disk, set()))
        (self.partition / "ro").write_text("0\n")
        (self.partition / "holders" / "dm-0").mkdir()
        self.assertFalse(data_volume.disk_available(self.disk, set()))

    def test_only_internal_sata_or_nvme_partitions_are_mount_candidates(self):
        self.assertTrue(data_volume.data_partition(self.partition))
        for bus in ("ata1", "usb2"):
            disk = self.root / bus / "block" / "sda"
            partition = disk / "sda1"
            partition.mkdir(parents=True)
            (disk / "removable").write_text("0\n")
            self.assertEqual(data_volume.data_partition(partition), bus == "ata1")
            (disk / "removable").write_text("1\n")
            self.assertFalse(data_volume.data_partition(partition))

    def test_mount_point_accepts_empty_packaged_directory_and_protects_files(self):
        data = self.root / "agx-data"
        fallback = data / "agx-opencode-users"
        fallback.mkdir(parents=True)
        lock = self.root / "lock"
        open_file = data_volume.os.open

        def open_lock(path, *args):
            self.assertEqual(path, "/run/agx-data-volume.lock")
            return open_file(lock, *args)

        with patch.object(data_volume, "DATA", data), patch.object(data_volume.os, "open", open_lock):
            with data_volume.mount_target():
                self.assertTrue(fallback.is_dir())
            (fallback / "account-file").write_text("keep")
            with self.assertRaisesRegex(RuntimeError, "mount point contains files"):
                with data_volume.mount_target():
                    self.fail("mount would hide the fallback data")
        self.assertEqual((fallback / "account-file").read_text(), "keep")

    def test_format_refuses_stale_device_and_mounts_the_new_uuid_on_success(self):
        @contextmanager
        def device(node, generation):
            self.assertEqual(generation, 7)
            yield str(node), 3

        def publish_partition(command, *args, **options):
            if command == data_volume.SFDISK:
                name = options["input"].split('"')[1]
                (self.partition / "uevent").write_text(f"PARTNAME={name}\n")

        (self.partition / "uevent").write_text("PARTNAME=Previous Partition\n")
        with (patch.object(data_volume, "BLOCK", self.root),
              patch.object(data_volume, "format_ready", return_value=True),
              patch.object(data_volume, "mount_target", side_effect=nullcontext),
              patch.object(data_volume, "devices_in_use", return_value=set()),
              patch.object(data_volume, "disk_device", side_effect=device),
              patch.object(data_volume, "command") as command,
              patch.object(data_volume, "mount_and_remember") as mount):
            with self.assertRaisesRegex(RuntimeError, "changed"):
                data_volume.format_and_mount("nvme0n1:6")
            command.assert_not_called()
            mount.assert_not_called()
            with self.assertRaisesRegex(RuntimeError, "new NVMe partition is unavailable"):
                data_volume.format_and_mount("nvme0n1:7")
            command.assert_called_once()
            mount.assert_not_called()
            command.reset_mock()
            command.side_effect = publish_partition
            data_volume.format_and_mount("nvme0n1:7")

        partitioning, formatting = command.call_args_list
        self.assertEqual(partitioning.args[0], data_volume.SFDISK)
        self.assertEqual(partitioning.args[-1], str(self.disk))
        self.assertEqual(formatting.args[0], data_volume.MKFS)
        self.assertEqual(formatting.args[-1], str(self.partition))
        uuid = formatting.args[formatting.args.index("-U") + 1]
        mount.assert_called_once_with(str(self.partition), uuid, pass_fds=(3,))


if __name__ == "__main__":
    unittest.main()
