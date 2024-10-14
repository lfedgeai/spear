package main

import (
	// cobra
	"github.com/lfedgeai/spear/worker"
	"github.com/spf13/cobra"
)

type WorkerConfig struct {
	Addr string
	Port string
}

func NewRootCmd() *cobra.Command {
	var rootCmd = &cobra.Command{
		Use:   "worker",
		Short: "Worker is the command line tool for the worker",
		Run: func(cmd *cobra.Command, args []string) {
			// parse flags
			addr, _ := cmd.Flags().GetString("addr")
			port, _ := cmd.Flags().GetString("port")

			// create config
			config := worker.NewWorkerConfig(addr, port)
			w := worker.NewWorker(config)
			w.Init()
			w.Run()
		},
	}

	// addr flag
	rootCmd.PersistentFlags().String("addr", "localhost", "address of the server")
	// port flag
	rootCmd.PersistentFlags().String("port", "8080", "port of the server")

	return rootCmd
}

func main() {
	NewRootCmd().Execute()
}
