# -*- coding: utf-8 -*-
# Generated by Django 1.10.3 on 2017-01-01 13:39
from __future__ import unicode_literals

from django.db import migrations, models


class Migration(migrations.Migration):

    dependencies = [
        ('wallet', '0004_sessionkey'),
    ]

    operations = [
        migrations.AddField(
            model_name='sessionkey',
            name='session_id',
            field=models.CharField(default='', max_length=20),
            preserve_default=False,
        ),
    ]