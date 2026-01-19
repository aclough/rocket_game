#!/bin/sh

sudo apt install flatpak
flatpak install flathub org.godotengine.Godot
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.godotengine.Godot
